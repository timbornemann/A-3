//! Apply one settled HTTP correction; never activates a step or dispatches a tool.

use crate::{
    AgentActionStore, AgentActionStoreFuture, AgentControllerControl, AgentSourceReadControl,
    AgentSourceReader, ApplyAgentPlanRevision, ApplyAgentPlanRevisionError,
    HttpCorrectionPlanAnchor, HttpCorrectionPlanDecision, HttpCorrectionPlanIntent,
    HttpCorrectionPlanStore, PreparedHttpContextIntent, PreparedHttpCorrectionPlan,
    TaskLedgerStoreVersion,
};
use a3_domain::{
    AgentControllerState, AgentFileStartLine, AgentRun, AgentRunTimestamp, ContentHash,
    DiffInvariantMode, DiffInvariantVerification, ExpectedTaskEvidence, GoalContract,
    GoalOriginalCoverage, IndexRunId, ProjectIdentity, PublishedIndex, RunEvent, RunEventId,
    RunEventSequence, StepDependency, TaskLedger, TaskLedgerReplanCause, TaskReplanReason,
    TaskStepDefinition, TaskStepId, TaskStepOutcome, TaskStepRationale, VerificationRequirement,
    VerificationSpec, VerificationSpecId, VerificationTarget,
};
use std::{error::Error, fmt, time::Instant};

use crate::model_wire::phase_elapsed_timestamp;

/// Candidate-only material decomposition of an observed failed HTTP proof.
#[derive(Clone, Copy)]
pub struct ApplyHttpCorrectionPlan<'a> {
    plans: &'a dyn HttpCorrectionPlanStore,
    actions: &'a dyn AgentActionStore,
    reader: &'a dyn AgentSourceReader,
}

impl<'a> ApplyHttpCorrectionPlan<'a> {
    /// Uses the existing current proposal, safe reader and atomic Ledger/Run boundaries.
    #[must_use]
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

    /// Applies only a genuine accepted V4 Focus at the unchanged original failure tail F.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute<C>(
        self,
        project: &ProjectIdentity,
        goal: &GoalContract,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        version: TaskLedgerStoreVersion,
        current: &PublishedIndex,
        phase: &PreparedHttpCorrectionPlan,
        observed_at: AgentRunTimestamp,
        control: &C,
    ) -> Result<HttpCorrectionPlanApplied, HttpCorrectionPlanApplyFailure>
    where
        C: AgentControllerControl + AgentSourceReadControl,
    {
        use HttpCorrectionPlanApplyFailure as E;
        let started = Instant::now();
        if AgentControllerControl::is_cancelled(control) {
            return Err(E::Cancelled);
        }
        let a = phase.anchor();
        ensure_current_application(
            project,
            goal,
            run,
            ledger,
            version,
            current,
            a,
            a.sequence,
            observed_at,
        )?;
        let proposal = self
            .plans
            .load_http_correction_plan_for_application(project, phase, observed_at)
            .await
            .map_err(|_| E::UnsettledOrStale)?
            .ok_or(E::UnsettledOrStale)?;
        let HttpCorrectionPlanDecision::FocusLine(focus) = proposal.decision() else {
            return Err(E::NoCorrectionIntent);
        };
        phase
            .revalidate_source(project, self.reader, control)
            .await
            .map_err(|_| E::SourceChanged)?;
        self.apply_material(
            project,
            goal,
            run,
            ledger,
            version,
            a,
            phase.job_digest(),
            focus,
            proposal.intent(),
            started,
            observed_at,
            control,
        )
        .await
    }

    /// Applies only a genuine settled Context Ready at actual read tail R; null remains a halt.
    /// Uses the same material definitions and Ledger/Run CAS as Direct, without a new model call.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_context<C>(
        self,
        project: &ProjectIdentity,
        goal: &GoalContract,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        version: TaskLedgerStoreVersion,
        current: &PublishedIndex,
        phase: &PreparedHttpContextIntent,
        observed_at: AgentRunTimestamp,
        control: &C,
    ) -> Result<HttpCorrectionPlanApplied, HttpCorrectionPlanApplyFailure>
    where
        C: AgentControllerControl + AgentSourceReadControl,
    {
        use HttpCorrectionPlanApplyFailure as E;
        let started = Instant::now();
        if AgentControllerControl::is_cancelled(control) {
            return Err(E::Cancelled);
        }
        let context = phase.anchor();
        let a = &context.parent;
        if a.sequence.get().checked_add(1) != Some(context.read_tail.get()) {
            return Err(E::Binding);
        }
        ensure_current_application(
            project,
            goal,
            run,
            ledger,
            version,
            current,
            a,
            context.read_tail,
            observed_at,
        )?;
        let settled = self
            .plans
            .load_http_context_intent_for_application(project, phase, observed_at)
            .await
            .map_err(|_| E::UnsettledOrStale)?
            .ok_or(E::UnsettledOrStale)?;
        let intent = settled.reply.intent().ok_or(E::NoCorrectionIntent)?;
        phase.encode_reply(&settled.reply).map_err(|_| E::Binding)?;
        // Freshness of both supplied originals, never a new acquisition or delivery credit.
        phase
            .revalidate_sources(project, self.reader, control)
            .await
            .map_err(|_| E::SourceChanged)?;
        self.apply_material(
            project,
            goal,
            run,
            ledger,
            version,
            a,
            phase.job_digest(),
            phase.core_focus(),
            intent,
            started,
            observed_at,
            control,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn apply_material<C>(
        self,
        project: &ProjectIdentity,
        goal: &GoalContract,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        version: TaskLedgerStoreVersion,
        a: &HttpCorrectionPlanAnchor,
        job: ContentHash,
        focus: AgentFileStartLine,
        intent: &HttpCorrectionPlanIntent,
        started: Instant,
        observed_at: AgentRunTimestamp,
        control: &C,
    ) -> Result<HttpCorrectionPlanApplied, HttpCorrectionPlanApplyFailure>
    where
        C: AgentControllerControl + AgentSourceReadControl,
    {
        use HttpCorrectionPlanApplyFailure as E;
        if AgentControllerControl::is_cancelled(control) {
            return Err(E::Cancelled);
        }
        let failed = ledger.step(a.step).ok_or(E::Binding)?.definition();
        crate::AdmitSourceTaskStep
            .execute(goal, failed)
            .map_err(|_| E::Binding)?;
        let VerificationTarget::LoopbackHttp(recipe) = failed.verification_spec().target() else {
            return Err(E::Binding);
        };
        if recipe.script() != a.source.path()
            || failed.acceptance_criteria().is_empty()
            || !failed.addressed_criteria().is_empty()
            || failed.parent_step_id().is_some()
            || !failed.dependencies().is_empty()
        {
            // Deliberately closed first slice: do not silently discard arbitrary graph prerequisites.
            return Err(E::UnsupportedGraph);
        }
        // Count the actual retained failed HTTP chain, never IDs or all ErrorCorrection entries.
        // A later mechanical retirement of a stale Add does not grant another HTTP correction.
        if ledger.replans().iter().any(|revision| {
            revision.cause() == TaskLedgerReplanCause::ErrorCorrection
                && revision.retired_step_ids().iter().any(|id| {
                    ledger.step(*id).is_some_and(|step| {
                        matches!(step.definition().verification_spec().target(),
                            VerificationTarget::LoopbackHttp(previous) if previous.id() == recipe.id())
                    })
                })
        }) {
            return Err(E::CorrectionConsumed);
        }
        let material = derive_http_correction_material_plan(a, job, failed, focus, intent)?;
        let (change, check, event_id) = material.into_parts();
        let correction = change.id();
        let proof = check.id();
        crate::AdmitSourceTaskStep
            .execute(goal, &change)
            .map_err(|_| E::Binding)?;
        crate::AdmitSourceTaskStep
            .execute(goal, &check)
            .map_err(|_| E::Binding)?;
        // Proposal loading and safe-source revalidation may consume the original Run deadline.
        // Never backdate the Ledger/Journal commit to the timestamp supplied before those awaits.
        let commit_at = phase_elapsed_timestamp(started, observed_at).ok_or(E::Binding)?;
        if run
            .budget_exhaustion(commit_at)
            .map_err(|_| E::Binding)?
            .is_some()
        {
            return Err(E::BudgetExhausted);
        }
        let current_actions = CurrentPublishedPlanActions {
            inner: self.actions,
            index: a.index,
        };
        let version = ApplyAgentPlanRevision::new(&current_actions)
            .execute(
                project,
                version,
                run,
                ledger,
                vec![a.step],
                vec![change, check],
                TaskReplanReason::try_from_string(
                    "Separate the accepted source correction from the unchanged HTTP proof"
                        .to_owned(),
                )
                .map_err(|_| E::Binding)?,
                event_id,
                commit_at,
                control,
            )
            .await
            .map_err(E::Replan)?;
        Ok(HttpCorrectionPlanApplied {
            correction,
            proof,
            version,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn ensure_current_application(
    project: &ProjectIdentity,
    goal: &GoalContract,
    run: &AgentRun,
    ledger: &TaskLedger,
    version: TaskLedgerStoreVersion,
    current: &PublishedIndex,
    a: &HttpCorrectionPlanAnchor,
    tail: RunEventSequence,
    observed_at: AgentRunTimestamp,
) -> Result<(), HttpCorrectionPlanApplyFailure> {
    use HttpCorrectionPlanApplyFailure as E;
    if run.state() != AgentControllerState::Replan
        || run.id() != a.run
        || run.goal_contract() != a.goal
        || goal.reference() != a.goal
        || ledger.goal_contract() != a.goal
        || goal.original().map(|original| original.digest()) != Some(a.original)
        || !GoalOriginalCoverage::derive_from(goal)
            .map_err(|_| E::Binding)?
            .is_complete()
        || ledger.revision() != a.ledger
        || run.task_ledger_revision() != a.ledger
        || run.model_profile() != Some(a.profile)
        || version != a.version
        || run.last_event_sequence() != tail
        || project.worktree().id() != a.worktree
        || current.run().id() != a.index
        || current.run().snapshot_id() != a.snapshot
        || run.current_snapshot_id() != a.snapshot
        || observed_at < run.updated_at()
        || ledger.pending_planning().is_some()
    {
        return Err(E::Binding);
    }
    if run
        .budget_exhaustion(observed_at)
        .map_err(|_| E::Binding)?
        .is_some()
    {
        return Err(E::BudgetExhausted);
    }
    Ok(())
}

/// Exact deterministic material definitions derived from one canonical retained HTTP job.
/// These definitions carry no permission to activate a step, read source or execute a tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpCorrectionMaterialPlan {
    change: TaskStepDefinition,
    proof: TaskStepDefinition,
    event: RunEventId,
    activation: [RunEventId; 3],
}
impl HttpCorrectionMaterialPlan {
    /// Exact Addresses correction definition.
    pub const fn change(&self) -> &TaskStepDefinition {
        &self.change
    }
    /// Exact dependent unchanged HTTP Proves definition.
    pub const fn proof(&self) -> &TaskStepDefinition {
        &self.proof
    }
    /// Exact LedgerUpdated identity.
    pub const fn event_id(&self) -> RunEventId {
        self.event
    }
    /// Ordered ReplanApplied, LocalizationComplete and PlanReady identities.
    pub const fn activation_events(&self) -> &[RunEventId; 3] {
        &self.activation
    }
    /// Moves the definitions into the existing plan revision use case.
    pub fn into_parts(self) -> (TaskStepDefinition, TaskStepDefinition, RunEventId) {
        (self.change, self.proof, self.event)
    }
}

/// Reconstructs only the closed isolated failed-proof decomposition, without source or store I/O.
/// The caller must independently establish the canonical job, current Goal and immutable failure.
pub fn derive_http_correction_material_plan(
    anchor: &HttpCorrectionPlanAnchor,
    job: ContentHash,
    failed: &TaskStepDefinition,
    focus: AgentFileStartLine,
    intent: &HttpCorrectionPlanIntent,
) -> Result<HttpCorrectionMaterialPlan, HttpCorrectionPlanApplyFailure> {
    use HttpCorrectionPlanApplyFailure as E;
    let VerificationTarget::LoopbackHttp(recipe) = failed.verification_spec().target() else {
        return Err(E::Binding);
    };
    if failed.id() != anchor.step || failed.verification_spec().id() != anchor.verification {
        return Err(E::Binding);
    }
    if recipe.script() != anchor.source.path()
        || failed.acceptance_criteria().is_empty()
        || !failed.addressed_criteria().is_empty()
        || failed.parent_step_id().is_some()
        || !failed.dependencies().is_empty()
    {
        return Err(E::UnsupportedGraph);
    }
    let id = |role: u8| {
        let mut hash = blake3::Hasher::new_derive_key("a3.http-correction-material-plan.v2");
        hash.update(job.as_bytes());
        hash.update(&focus.get().to_le_bytes());
        hash.update(&(intent.as_str().len() as u64).to_le_bytes());
        hash.update(intent.as_str().as_bytes());
        hash.update(&[role]);
        *hash.finalize().as_bytes()
    };
    let correction = TaskStepId::from_bytes(id(1));
    let proof = TaskStepId::from_bytes(id(2));
    let change = TaskStepDefinition::new(
        correction,
        None,
        TaskStepOutcome::try_from_string(intent.as_str().to_owned()).map_err(|_| E::Binding)?,
        TaskStepRationale::try_from_string(
            "Implement the model plan intent; the unchanged HTTP proof remains open".to_owned(),
        )
        .map_err(|_| E::Binding)?,
        Vec::new(),
        vec![
            ExpectedTaskEvidence::try_from_string("Current exact-path diff".to_owned())
                .map_err(|_| E::Binding)?,
        ],
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes(id(3)),
            VerificationRequirement::try_from_string(
                "Change only the bound HTTP source".to_owned(),
            )
            .map_err(|_| E::Binding)?,
            DiffInvariantVerification::new(
                DiffInvariantMode::ExactPaths,
                vec![anchor.source.path().clone()],
            )
            .map_err(|_| E::Binding)?,
        ),
    )
    .and_then(|step| {
        step.with_criterion_coverage(failed.acceptance_criteria().to_vec(), Vec::new())
    })
    .map_err(|_| E::Binding)?;
    let check = TaskStepDefinition::new(
        proof,
        None,
        TaskStepOutcome::try_from_string("Verify the complete original HTTP recipe".to_owned())
            .map_err(|_| E::Binding)?,
        failed.rationale().clone(),
        vec![StepDependency::new(correction)],
        failed.expected_evidence().to_vec(),
        VerificationSpec::loopback_http(
            VerificationSpecId::from_bytes(id(4)),
            failed.verification_spec().requirement().clone(),
            recipe.clone(),
        ),
    )
    .and_then(|step| step.with_acceptance_criteria(failed.acceptance_criteria().to_vec()))
    .map_err(|_| E::Binding)?;
    Ok(HttpCorrectionMaterialPlan {
        change,
        proof: check,
        event: RunEventId::from_bytes(id(5)),
        activation: [
            RunEventId::from_bytes(id(6)),
            RunEventId::from_bytes(id(7)),
            RunEventId::from_bytes(id(8)),
        ],
    })
}

impl fmt::Debug for ApplyHttpCorrectionPlan<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApplyHttpCorrectionPlan")
            .finish_non_exhaustive()
    }
}

// The exact index already checked against the immutable phase anchor must remain current
// inside the same transaction as both Ledger/Run CASes, including same-snapshot publication.
#[derive(Debug)]
struct CurrentPublishedPlanActions<'a> {
    inner: &'a dyn AgentActionStore,
    index: IndexRunId,
}
impl AgentActionStore for CurrentPublishedPlanActions<'_> {
    fn commit_ledger_action<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        version: TaskLedgerStoreVersion,
        sequence: RunEventSequence,
        ledger: &'a TaskLedger,
        run: &'a AgentRun,
        event: &'a RunEvent,
    ) -> AgentActionStoreFuture<'a> {
        self.inner.commit_current_index_ledger_action(
            project, version, sequence, self.index, ledger, run, event,
        )
    }
}

/// Persisted IDs only; no execution or source-read capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpCorrectionPlanApplied {
    /// Exact-path Addresses step that remains unstarted.
    pub correction: TaskStepId,
    /// Dependent complete original HTTP Proves step that remains unstarted.
    pub proof: TaskStepId,
    /// Resulting persisted Ledger CAS version.
    pub version: TaskLedgerStoreVersion,
}

/// Closed application failure before any step activation or tool effect.
#[derive(Debug)]
pub enum HttpCorrectionPlanApplyFailure {
    /// Cancellation won before the plan revision commit.
    Cancelled,
    /// Current immutable Goal, Run, Ledger, source or job coordinates disagree.
    Binding,
    /// The original Run budget permits no further core work.
    BudgetExhausted,
    /// No fully settled current proposal is available for this exact job.
    UnsettledOrStale,
    /// The legal model outcome requests evidence, declares capacity or refuses.
    NoCorrectionIntent,
    /// A fresh safe read did not reproduce the exact supplied source bytes.
    SourceChanged,
    /// The bounded first slice cannot preserve this graph's prerequisites.
    UnsupportedGraph,
    /// This original HTTP failure chain already received its material correction.
    CorrectionConsumed,
    /// The existing atomic Ledger/Run revision boundary rejected the update.
    Replan(ApplyAgentPlanRevisionError),
}
impl fmt::Display for HttpCorrectionPlanApplyFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HTTP correction plan could not be applied")
    }
}
impl Error for HttpCorrectionPlanApplyFailure {}
