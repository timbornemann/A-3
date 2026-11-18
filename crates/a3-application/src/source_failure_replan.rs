//! Material correction after the candidate recorded an actual guarded red test.

use crate::{
    AdmitSourceTaskStep, AgentActionStore, AgentControllerControl, ApplyAgentPlanRevision,
    ApplyAgentPlanRevisionError, RunEventPageLimit, RunJournalStore, RunJournalStoreFailure,
    TaskLedgerStoreVersion,
};
use a3_domain::{
    AgentControllerState, AgentRun, AgentRunId, AgentRunTimestamp, DiffInvariantMode,
    DiffInvariantVerification, ExpectedTaskEvidence, FileRevision, GoalContract,
    GoalContractReference, ProjectIdentity, PublishedIndex, RunEvent, RunEventCode, RunEventId,
    RunEventKind, RunEventOutcome, RunEventRedactionSource, RunEventSubject, SnapshotId,
    StepDependency, TaskLedger, TaskLedgerReplanCause, TaskLedgerRevision, TaskReplanReason,
    TaskStep, TaskStepAttemptNumber, TaskStepAttemptOutcome, TaskStepDefinition,
    TaskStepFailureReason, TaskStepId, TaskStepOutcome, TaskStepRationale, TaskStepStatus,
    VerificationRequirement, VerificationSpec, VerificationSpecId, VerificationTarget,
};
use std::{error::Error, fmt};

const MAX_FAILURE_SUMMARY_BYTES: usize = 256;

/// Retires one failed combined edit/test step and installs separate correction and proof steps.
#[derive(Debug, Clone, Copy)]
pub struct ApplySourceTestCorrection<'a> {
    actions: &'a dyn AgentActionStore,
    journal: &'a dyn RunJournalStore,
}

impl<'a> ApplySourceTestCorrection<'a> {
    /// Uses the existing atomic Ledger/Run action boundary and append-only journal.
    #[must_use]
    pub const fn new(actions: &'a dyn AgentActionStore, journal: &'a dyn RunJournalStore) -> Self {
        Self { actions, journal }
    }

    /// Installs the first correction on the same run without spending or resetting model budget.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        goal: &GoalContract,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        version: TaskLedgerStoreVersion,
        current: &PublishedIndex,
        observed_at: AgentRunTimestamp,
        control: &dyn AgentControllerControl,
    ) -> Result<SourceTestCorrection, SourceTestCorrectionFailure> {
        use SourceTestCorrectionFailure as Failure;

        if control.is_cancelled() {
            return Err(Failure::Cancelled);
        }
        let snapshot = validate_binding(goal, run, ledger, current)?;
        if run.state() != AgentControllerState::Replan
            || !ledger.replans().is_empty()
            || ledger.steps().len() != 1
        {
            return Err(Failure::InvalidState);
        }
        if run
            .budget_exhaustion(observed_at)
            .map_err(|_| Failure::InvalidState)?
            .is_some()
        {
            return Err(Failure::BudgetExhausted);
        }
        let events = self.load_journal(project, run).await?;
        let (red, next) = failure_receipt(&events, snapshot)?;
        if next.is_some() {
            return Err(Failure::InvalidFailureReceipt);
        }
        let step = ledger.steps().next().ok_or(Failure::InvalidState)?;
        if !step.is_active_plan_step() {
            return Err(Failure::InvalidState);
        }
        failure_attempt(step, red)?;
        let plan = correction_plan(goal, step, red, current)?;
        let failed_step_id = step.definition().id();
        let change_step_id = plan.change.id();
        let proof_step_id = plan.proof.id();
        let reason = TaskReplanReason::try_from_string(
            "Separate the source correction from its unchanged guarded test after the red result"
                .to_owned(),
        )
        .map_err(|_| Failure::InvalidPlan)?;
        let version = ApplyAgentPlanRevision::new(self.actions)
            .execute(
                project,
                version,
                run,
                ledger,
                vec![failed_step_id],
                vec![plan.change, plan.proof],
                reason,
                plan.event_id,
                observed_at,
                control,
            )
            .await
            .map_err(Failure::Replan)?;
        Ok(SourceTestCorrection {
            change_step_id,
            proof_step_id,
            ledger_version: version,
        })
    }

    /// Restores the bounded diagnosis for the exact Ready or executing correction Edit.
    pub async fn load_edit_diagnostic(
        self,
        project: &ProjectIdentity,
        goal: &GoalContract,
        run: &AgentRun,
        ledger: &TaskLedger,
        current: &PublishedIndex,
        change_step_id: TaskStepId,
    ) -> Result<SourceFailureEditDiagnostic, SourceTestCorrectionFailure> {
        use SourceTestCorrectionFailure as Failure;

        let snapshot = validate_binding(goal, run, ledger, current)?;
        let [revision] = ledger.replans() else {
            return Err(Failure::InvalidState);
        };
        let [failed_step_id] = revision.retired_step_ids() else {
            return Err(Failure::InvalidPlan);
        };
        if revision.cause() != TaskLedgerReplanCause::ErrorCorrection
            || revision.revision() != ledger.revision()
            || ledger.steps().len() != 3
        {
            return Err(Failure::InvalidPlan);
        }
        let failed = ledger.step(*failed_step_id).ok_or(Failure::InvalidPlan)?;
        let change = ledger.step(change_step_id).ok_or(Failure::InvalidPlan)?;
        if failed.retired_in_revision() != Some(revision.revision())
            || !change.is_active_plan_step()
            || !match (run.state(), change.status()) {
                (AgentControllerState::Replan, TaskStepStatus::Ready) => {
                    change.attempts().is_empty()
                }
                (AgentControllerState::Execute, TaskStepStatus::InProgress) => {
                    matches!(change.attempts(), [attempt] if attempt.run_id() == run.id()
                        && matches!(attempt.outcome(), TaskStepAttemptOutcome::Active))
                }
                _ => false,
            }
        {
            return Err(Failure::InvalidState);
        }

        let events = self.load_journal(project, run).await?;
        let (red, next) = failure_receipt(&events, snapshot)?;
        let (number, reason) = failure_attempt(failed, red)?;
        let plan = correction_plan(goal, failed, red, current)?;
        let proof = ledger.step(plan.proof.id()).ok_or(Failure::InvalidPlan)?;
        if change.definition() != &plan.change
            || proof.definition() != &plan.proof
            || change.introduced_in_revision() != revision.revision()
            || proof.introduced_in_revision() != revision.revision()
            || !proof.is_active_plan_step()
            || revision.added_step_ids().len() != 2
            || !revision.added_step_ids().contains(&plan.change.id())
            || !revision.added_step_ids().contains(&plan.proof.id())
            || !next.is_some_and(|event| {
                event.id() == plan.event_id
                    && event.snapshot_id() == snapshot
                    && matches!(event.kind(), RunEventKind::LedgerUpdated { from, to }
                        if from == revision.previous_revision() && to == revision.revision())
                    && event.payload().code() == RunEventCode::ControllerDecision
                    && event.payload().outcome() == Some(RunEventOutcome::Succeeded)
            })
        {
            return Err(Failure::InvalidPlan);
        }
        Ok(SourceFailureEditDiagnostic {
            goal: goal.reference(),
            run_id: run.id(),
            ledger_revision: ledger.revision(),
            snapshot_id: snapshot,
            change_step_id,
            failed_step_id: *failed_step_id,
            failed_attempt_number: number,
            source_revision: plan.source_revision,
            reason: reason.clone(),
        })
    }

    async fn load_journal(
        self,
        project: &ProjectIdentity,
        run: &AgentRun,
    ) -> Result<Vec<RunEvent>, SourceTestCorrectionFailure> {
        use SourceTestCorrectionFailure as Failure;

        // A red receipt can land immediately after the admission page. A second
        // page also covers the bounded correction and activation events.
        const MAX_PAGES: u64 = 2;
        let limit = RunEventPageLimit::new(256).map_err(|_| Failure::InvalidFailureReceipt)?;
        let tail = run.last_event_sequence();
        if tail.get() > MAX_PAGES * u64::from(limit.get()) {
            return Err(Failure::InvalidFailureReceipt);
        }
        let mut events = Vec::new();
        for _ in 0..MAX_PAGES {
            let after = events.last().map(RunEvent::sequence);
            let first = after.map_or(1, |sequence| sequence.get() + 1);
            let page = self
                .journal
                .load_run_events(project, run.id(), after, limit)
                .await
                .map_err(Failure::Journal)?;
            if page.events().first().map(|event| event.sequence().get()) != Some(first)
                || page.events().iter().any(|event| event.run_id() != run.id())
            {
                return Err(Failure::InvalidFailureReceipt);
            }
            events.extend_from_slice(page.events());
            if !page.has_more() {
                return if events.last().map(RunEvent::sequence) == Some(tail) {
                    Ok(events)
                } else {
                    Err(Failure::InvalidFailureReceipt)
                };
            }
        }
        Err(Failure::InvalidFailureReceipt)
    }
}

fn validate_binding(
    goal: &GoalContract,
    run: &AgentRun,
    ledger: &TaskLedger,
    current: &PublishedIndex,
) -> Result<SnapshotId, SourceTestCorrectionFailure> {
    use SourceTestCorrectionFailure as Failure;

    if goal.original().is_none()
        || goal.reference() != run.goal_contract()
        || goal.reference() != ledger.goal_contract()
        || run.task_ledger_revision() != ledger.revision()
        || ledger.pending_planning().is_some()
    {
        return Err(Failure::InvalidState);
    }
    let snapshot = current.run().snapshot_id();
    if snapshot != run.current_snapshot_id() {
        return Err(Failure::SnapshotChanged);
    }
    Ok(snapshot)
}

fn failure_receipt(
    events: &[RunEvent],
    snapshot: SnapshotId,
) -> Result<(&RunEvent, Option<&RunEvent>), SourceTestCorrectionFailure> {
    use SourceTestCorrectionFailure as Failure;

    let mut red = events.iter().enumerate().filter(|(_, event)| {
        matches!(
            event.kind(),
            RunEventKind::StateTransition {
                from: AgentControllerState::Execute,
                to: AgentControllerState::Replan,
            }
        ) && event.payload().code() == RunEventCode::VerificationFailure
            && event.payload().outcome() == Some(RunEventOutcome::Failed)
    });
    let (index, transition) = red.next().ok_or(Failure::InvalidFailureReceipt)?;
    let tool = index
        .checked_sub(1)
        .and_then(|previous| events.get(previous))
        .ok_or(Failure::InvalidFailureReceipt)?;
    if red.next().is_some()
        || transition.snapshot_id() != snapshot
        || tool.snapshot_id() != snapshot
        || tool.occurred_at() != transition.occurred_at()
        || tool.kind() != RunEventKind::ToolAction
        || !matches!(tool.subject(), Some(RunEventSubject::Tool(_)))
        || tool.payload().outcome() != Some(RunEventOutcome::Succeeded)
        || !tool.payload().redaction().is_some_and(|metadata| {
            metadata.source() == RunEventRedactionSource::ToolOutput
                && metadata.observed_bytes() > 0
                && !metadata.source_was_truncated()
        })
    {
        return Err(Failure::InvalidFailureReceipt);
    }
    Ok((transition, events.get(index + 1)))
}

fn failure_attempt<'step>(
    step: &'step TaskStep,
    red: &RunEvent,
) -> Result<(TaskStepAttemptNumber, &'step TaskStepFailureReason), SourceTestCorrectionFailure> {
    use SourceTestCorrectionFailure as Failure;

    let [attempt] = step.attempts() else {
        return Err(Failure::InvalidFailureReceipt);
    };
    let TaskStepAttemptOutcome::Failed { reason } = attempt.outcome() else {
        return Err(Failure::InvalidFailureReceipt);
    };
    if step.status() != TaskStepStatus::Failed
        || attempt.run_id() != red.run_id()
        || attempt.verification().is_some()
        || attempt
            .finished_at()
            .is_none_or(|at| at.unix_millis() != red.occurred_at().unix_millis())
        || reason.as_str().len() > MAX_FAILURE_SUMMARY_BYTES
    {
        return Err(Failure::InvalidFailureReceipt);
    }
    Ok((attempt.number(), reason))
}

struct CorrectionPlan {
    change: TaskStepDefinition,
    proof: TaskStepDefinition,
    event_id: RunEventId,
    source_revision: FileRevision,
}

fn correction_plan(
    goal: &GoalContract,
    failed: &TaskStep,
    red: &RunEvent,
    current: &PublishedIndex,
) -> Result<CorrectionPlan, SourceTestCorrectionFailure> {
    use SourceTestCorrectionFailure as Failure;

    let failed = failed.definition();
    let spec = failed.verification_spec();
    let VerificationTarget::Test {
        command_id,
        selector,
        minimum_cases,
        scope,
    } = spec.target()
    else {
        return Err(Failure::InvalidPlan);
    };
    let guard = spec
        .python_integer_translation()
        .ok_or(Failure::InvalidPlan)?;
    if failed.acceptance_criteria().is_empty()
        || !failed.addressed_criteria().is_empty()
        || !failed.dependencies().is_empty()
        || failed.parent_step_id().is_some()
    {
        return Err(Failure::InvalidPlan);
    }
    AdmitSourceTaskStep
        .execute(goal, failed)
        .map_err(|_| Failure::InvalidPlan)?;
    let source_revision = current
        .publication()
        .graph()
        .files()
        .iter()
        .find(|revision| revision.path() == guard.path())
        .ok_or(Failure::SnapshotChanged)?
        .clone();

    let id = |role| {
        let mut hash = blake3::Hasher::new_derive_key("a3 source failed test correction v1");
        hash.update(red.run_id().as_bytes());
        hash.update(red.id().as_bytes());
        hash.update(failed.id().as_bytes());
        hash.update(&[role]);
        *hash.finalize().as_bytes()
    };
    let change_id = TaskStepId::from_bytes(id(1));
    let invariant =
        DiffInvariantVerification::new(DiffInvariantMode::ExactPaths, vec![guard.path().clone()])
            .map_err(|_| Failure::InvalidPlan)?;
    let change = TaskStepDefinition::new(
        change_id,
        None,
        TaskStepOutcome::try_from_string(
            "Correct the current source implicated by the failed guarded test".to_owned(),
        )
        .map_err(|_| Failure::InvalidPlan)?,
        TaskStepRationale::try_from_string(
            "Address the real failure in the exact source file before running the proof".to_owned(),
        )
        .map_err(|_| Failure::InvalidPlan)?,
        Vec::new(),
        vec![
            ExpectedTaskEvidence::try_from_string("Current exact-path diff".to_owned())
                .map_err(|_| Failure::InvalidPlan)?,
        ],
        VerificationSpec::diff_invariant(
            VerificationSpecId::from_bytes(id(2)),
            VerificationRequirement::try_from_string(
                "Change only the guarded source file".to_owned(),
            )
            .map_err(|_| Failure::InvalidPlan)?,
            invariant,
        ),
    )
    .and_then(|step| {
        step.with_criterion_coverage(failed.acceptance_criteria().to_vec(), Vec::new())
    })
    .map_err(|_| Failure::InvalidPlan)?;
    let proof = TaskStepDefinition::new(
        TaskStepId::from_bytes(id(3)),
        None,
        TaskStepOutcome::try_from_string(
            "Run the original guarded test on the correction".to_owned(),
        )
        .map_err(|_| Failure::InvalidPlan)?,
        failed.rationale().clone(),
        vec![StepDependency::new(change_id)],
        failed.expected_evidence().to_vec(),
        VerificationSpec::test_with_python_integer_translation(
            VerificationSpecId::from_bytes(id(4)),
            spec.requirement().clone(),
            *command_id,
            selector.clone(),
            *minimum_cases,
            *scope,
            guard.clone(),
        ),
    )
    .and_then(|step| step.with_acceptance_criteria(failed.acceptance_criteria().to_vec()))
    .map_err(|_| Failure::InvalidPlan)?;
    Ok(CorrectionPlan {
        change,
        proof,
        event_id: RunEventId::from_bytes(id(5)),
        source_revision,
    })
}

/// Exact replacement step identities and resulting persisted Ledger version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTestCorrection {
    change_step_id: TaskStepId,
    proof_step_id: TaskStepId,
    ledger_version: TaskLedgerStoreVersion,
}

impl SourceTestCorrection {
    /// Returns the correction's exact-path Addresses step.
    #[must_use]
    pub const fn change_step_id(self) -> TaskStepId {
        self.change_step_id
    }

    /// Returns the dependent guarded Proves step.
    #[must_use]
    pub const fn proof_step_id(self) -> TaskStepId {
        self.proof_step_id
    }

    /// Returns the newly committed store CAS version.
    #[must_use]
    pub const fn ledger_version(self) -> TaskLedgerStoreVersion {
        self.ledger_version
    }
}

/// Bounded diagnosis reconstructed from the same red attempt and material correction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFailureEditDiagnostic {
    goal: GoalContractReference,
    run_id: AgentRunId,
    ledger_revision: TaskLedgerRevision,
    snapshot_id: SnapshotId,
    change_step_id: TaskStepId,
    failed_step_id: TaskStepId,
    failed_attempt_number: TaskStepAttemptNumber,
    source_revision: FileRevision,
    reason: TaskStepFailureReason,
}

impl SourceFailureEditDiagnostic {
    /// Returns the original immutable Goal revision.
    #[must_use]
    pub const fn goal(&self) -> GoalContractReference {
        self.goal
    }

    /// Returns the same Run that owned the failed attempt.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the material correction's Ledger revision.
    #[must_use]
    pub const fn ledger_revision(&self) -> TaskLedgerRevision {
        self.ledger_revision
    }

    /// Returns the unchanged snapshot of the actual red test.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the sole correction step that may use the diagnosis.
    #[must_use]
    pub const fn change_step_id(&self) -> TaskStepId {
        self.change_step_id
    }

    /// Returns the retired combined step for the actual red attempt.
    #[must_use]
    pub const fn failed_step_id(&self) -> TaskStepId {
        self.failed_step_id
    }

    /// Returns the exact immutable failed attempt number.
    #[must_use]
    pub const fn failed_attempt_number(&self) -> TaskStepAttemptNumber {
        self.failed_attempt_number
    }

    /// Returns the exact current source the correction must edit.
    #[must_use]
    pub const fn source_revision(&self) -> &FileRevision {
        &self.source_revision
    }

    /// Returns the retained normalized diagnostic for bounded model input.
    #[must_use]
    pub fn summary(&self) -> &str {
        self.reason.as_str()
    }
}

/// Closed failure at the candidate's material correction boundary.
#[derive(Debug)]
pub enum SourceTestCorrectionFailure {
    /// Cancellation won before the correction could be committed.
    Cancelled,
    /// The current Goal, Run, Ledger or correction stage does not match.
    InvalidState,
    /// The published snapshot or guarded file is no longer current.
    SnapshotChanged,
    /// The original Run budget permits no further work.
    BudgetExhausted,
    /// The bounded journal and failed step do not establish an actual eligible red result.
    InvalidFailureReceipt,
    /// The exact two-step correction cannot be constructed or restored.
    InvalidPlan,
    /// Reading the existing journal failed.
    Journal(RunJournalStoreFailure),
    /// The existing atomic plan revision boundary rejected the correction.
    Replan(ApplyAgentPlanRevisionError),
}

impl fmt::Display for SourceTestCorrectionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Cancelled => "source test correction was cancelled",
            Self::InvalidState => "source test correction does not match the current run",
            Self::SnapshotChanged => "source test correction snapshot changed",
            Self::BudgetExhausted => "source test correction run budget is exhausted",
            Self::InvalidFailureReceipt => "source test correction lacks a current red receipt",
            Self::InvalidPlan => "source test correction does not match the exact split",
            Self::Journal(_) => "source test correction journal is unavailable",
            Self::Replan(_) => "source test correction could not be committed",
        })
    }
}

impl Error for SourceTestCorrectionFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Journal(error) => Some(error),
            Self::Replan(error) => Some(error),
            _ => None,
        }
    }
}
