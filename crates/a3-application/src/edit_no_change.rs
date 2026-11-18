//! Close only a proven unchanged current Existing/Replace attempt; no new model job.
use crate::{
    AdvanceAgentController, AgentActionStore, AgentActionStoreFailure, AgentControllerControl,
    AgentControllerError, AgentControllerSignal, AgentSourcePage, EditPhaseAnchor,
    EditPhaseCallKind, EditPhaseFailure, EditPhaseObservation, EditPhaseObservationOutcome,
    EditPhaseReservation, EditPhaseReservationFailure, EditPhaseReservationStore, EditPhaseState,
    EditPhaseWireCost, PreparedEditPhase, StoredTaskLedger, TaskLedgerStoreVersion,
    WorkspacePatchControl, WorkspacePatchTool,
};
use a3_domain::{
    AgentControllerState, AgentRun, AgentRunTimestamp, ContentHash, EvidenceRef, FileRevision,
    GoalContract, GoalOriginalCoverage, GoalOriginalDigest, IndexRunId, ModelProfile,
    ModelProfileReference, ProjectIdentity, PublishedIndex, RunEvent, RunEventId, TaskLedger,
    TaskLedgerError, TaskLedgerTimestamp, TaskStepAttemptOutcome, TaskStepFailureReason,
    TaskStepStatus,
};
use std::{error::Error, fmt, time::Instant};

/// Descriptive bytes of one nonempty Existing/Replace window. It establishes no
/// actual dispatch, unchanged reply, source delivery or currentness by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditNoChangeWindow {
    revision: FileRevision,
    start: u32,
    end: u32,
    digest: ContentHash,
}
impl EditNoChangeWindow {
    pub(crate) fn from_window(
        revision: FileRevision,
        start: u32,
        end: u32,
        digest: ContentHash,
    ) -> Option<Self> {
        if start >= end || end - start > crate::edit_phase::MAX_EDIT_BYTES as u32 {
            return None;
        }
        Some(Self {
            revision,
            start,
            end,
            digest,
        })
    }
    /// Complete original file identity.
    pub const fn revision(&self) -> &FileRevision {
        &self.revision
    }
    /// First editable byte; no fuzzy source matching is allowed.
    pub const fn start_byte(&self) -> u32 {
        self.start
    }
    /// Exclusive last editable byte.
    pub const fn end_byte(&self) -> u32 {
        self.end
    }
    /// Digest of the exact originally supplied replacement bytes.
    pub const fn digest(&self) -> ContentHash {
        self.digest
    }
    /// Exact Original interval; containment or overlapping source is insufficient.
    pub fn matches_original(&self, original: &EvidenceRef) -> bool {
        original.revision() == &self.revision
            && original.range().start_byte() == self.start
            && original.range().end_byte() == self.end
    }
}

/// Store-authenticated terminal fact, not a successful source-delivery receipt or
/// product VerificationEvidence. Its constructor checks shape, not database truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditNoChangeReceipt {
    anchor: EditPhaseAnchor,
    original_goal: GoalOriginalDigest,
    job: ContentHash,
    window: EditNoChangeWindow,
    original: EvidenceRef,
    source_read: ContentHash,
    index: IndexRunId,
    profile: ModelProfileReference,
    generation_limit: u32,
    context_limit: u32,
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
    reserved_at: AgentRunTimestamp,
    finished_at: AgentRunTimestamp,
    deadline: AgentRunTimestamp,
    reported_prompt: u32,
    reported_output: u32,
    request_digest: Option<ContentHash>,
}
impl EditNoChangeReceipt {
    /// Adapter constructor after exact immutable row, actual Original/ToolAction,
    /// initial/repair history, full profile and live owner/index authentication.
    /// A generic rejected_reply without its dedicated typed record is insufficient.
    pub fn from_recorded(
        phase: &PreparedEditPhase,
        permit: &EditPhaseReservation,
        observation: EditPhaseObservation,
        original: EvidenceRef,
        profile: &ModelProfile,
        current: &PublishedIndex,
        original_goal: GoalOriginalDigest,
    ) -> Result<Self, EditPhaseReservationFailure> {
        use EditPhaseReservationFailure as E;
        let window = phase.no_change_window().ok_or(E::InvalidStoredData)?;
        let source_read = permit.source_read_key().ok_or(E::InvalidStoredData)?;
        let prompt = observation
            .reported_prompt_tokens()
            .filter(|n| *n > 0)
            .ok_or(E::IncompleteProviderUsage)?;
        let output = observation
            .reported_output_tokens()
            .filter(|n| *n > 0)
            .ok_or(E::IncompleteProviderUsage)?;
        let context = profile.settings().context_limit().get();
        let generation = profile.settings().output_limit().get();
        let reserve = u32::try_from((u64::from(context) * 22).div_ceil(100))
            .map_err(|_| E::InvalidStoredData)?
            .max(generation);
        if permit.anchor() != phase.anchor()
            || permit.job_digest() != phase.job_digest()
            || observation.outcome() != EditPhaseObservationOutcome::NoChange
            || !window.matches_original(&original)
            || !profile.executable_actions_enabled()
            || profile.reference() != phase.anchor().model_profile()
            || phase.anchor().snapshot() != current.run().snapshot_id()
            || !current
                .publication()
                .graph()
                .files()
                .contains(window.revision())
            || observation.finished_at() < permit.reserved_at()
            || observation.finished_at() >= permit.deadline()
            || prompt > permit.wire().prompt_estimate()
            || output > generation
            || permit.wire().output_reserve() != reserve
            || permit
                .wire()
                .prompt_estimate()
                .checked_add(reserve)
                .is_none_or(|sum| sum > context)
        {
            return Err(E::InvalidStoredData);
        }
        Ok(Self {
            anchor: phase.anchor(),
            original_goal,
            job: phase.job_digest(),
            window,
            original,
            source_read,
            index: current.run().id(),
            profile: profile.reference(),
            generation_limit: generation,
            context_limit: context,
            kind: permit.kind(),
            wire: permit.wire(),
            reserved_at: permit.reserved_at(),
            finished_at: observation.finished_at(),
            deadline: permit.deadline(),
            reported_prompt: prompt,
            reported_output: output,
            request_digest: permit.request_digest(),
        })
    }
    /// Actual Edit3 request identity joined by the Store, never a parent-pass or execution permit.
    /// None describes only a genuine legacy receipt with no Edit3 request row.
    pub const fn request_digest(&self) -> Option<ContentHash> {
        self.request_digest
    }

    /// Exact current step/attempt/Goal/Ledger owner.
    pub const fn anchor(&self) -> EditPhaseAnchor {
        self.anchor
    }
    /// Exact immutable original retained at the observed dispatch's Goal revision.
    pub const fn original_goal(&self) -> GoalOriginalDigest {
        self.original_goal
    }
    /// Original logical job; a repair remains this same job.
    pub const fn job_digest(&self) -> ContentHash {
        self.job
    }
    /// Exact current replacement byte descriptor.
    pub const fn window(&self) -> &EditNoChangeWindow {
        &self.window
    }
    /// Actual successfully read Original, with no accepted-delivery credit.
    pub const fn original(&self) -> &EvidenceRef {
        &self.original
    }
    /// Existing charged source acquisition identity.
    pub const fn source_read_key(&self) -> ContentHash {
        self.source_read
    }
    /// Exact publication, including a changed index at an identical snapshot.
    pub const fn index(&self) -> IndexRunId {
        self.index
    }
    /// Immutable model profile identity authenticated with the full profile.
    pub const fn profile(&self) -> ModelProfileReference {
        self.profile
    }
    /// Actual generation cap, distinct from the22-percent reservation.
    pub const fn generation_limit(&self) -> u32 {
        self.generation_limit
    }
    /// Actual original configured context, never inferred from a model name.
    pub const fn context_limit(&self) -> u32 {
        self.context_limit
    }
    /// The actual initial or one existing structural-repair dispatch.
    pub const fn kind(&self) -> EditPhaseCallKind {
        self.kind
    }
    /// Actual wire identity and charged reservations.
    pub const fn wire(&self) -> EditPhaseWireCost {
        self.wire
    }
    /// Original reservation timestamp.
    pub const fn reserved_at(&self) -> AgentRunTimestamp {
        self.reserved_at
    }
    /// Durable terminal observation including owned Store I/O; never a parent-pass timestamp.
    pub const fn finished_at(&self) -> AgentRunTimestamp {
        self.finished_at
    }
    /// Original unchanged absolute Run deadline.
    pub const fn deadline(&self) -> AgentRunTimestamp {
        self.deadline
    }
    /// Positive observed prompt usage; charges are not refunded.
    pub const fn reported_prompt(&self) -> u32 {
        self.reported_prompt
    }
    /// Positive observed output usage within the actual generation cap.
    pub const fn reported_output(&self) -> u32 {
        self.reported_output
    }
    fn event_id(&self) -> RunEventId {
        no_change_closure_event_id(
            self.anchor.run(),
            self.job,
            self.wire,
            self.source_read,
            self.index,
            self.kind,
        )
    }
}

// Same existing A-event bytes for live closure and descriptive closed lineage.
// This function grants no permission to construct a live EditPhaseAnchor.
pub(crate) fn no_change_closure_event_id(
    run: a3_domain::AgentRunId,
    job: ContentHash,
    wire: EditPhaseWireCost,
    source_read: ContentHash,
    index: IndexRunId,
    kind: EditPhaseCallKind,
) -> RunEventId {
    let mut hash = blake3::Hasher::new_derive_key("a3.edit.no-change-close.v1");
    hash.update(run.as_bytes());
    hash.update(job.as_bytes());
    hash.update(wire.digest().as_bytes());
    hash.update(source_read.as_bytes());
    hash.update(index.as_bytes());
    hash.update(&[match kind {
        EditPhaseCallKind::Initial => 0,
        EditPhaseCallKind::StructuralRepair => 1,
    }]);
    RunEventId::from_bytes(*hash.finalize().as_bytes())
}

/// Exact deterministic expected aggregates for the existing atomic closure CAS.
#[derive(Debug)]
pub struct EditNoChangeClosure {
    ledger: TaskLedger,
    run: AgentRun,
    event: RunEvent,
}
impl EditNoChangeClosure {
    /// Same revision, with only the active attempt closed Failed.
    pub const fn ledger(&self) -> &TaskLedger {
        &self.ledger
    }
    /// Same Run and budgets, advanced by the existing Execute→Replan edge.
    pub const fn run(&self) -> &AgentRun {
        &self.run
    }
    /// The one append-only deterministic state-transition event.
    pub const fn event(&self) -> &RunEvent {
        &self.event
    }
    /// Moves verified copies to the caller only after its CAS succeeded.
    pub fn into_parts(self) -> (TaskLedger, AgentRun, RunEvent) {
        (self.ledger, self.run, self.event)
    }
}

/// Pure expected closure. Receipt authentication, source freshness, cancellation,
/// cumulative phase charges and the final exact index pin remain caller/store duties.
pub fn derive_edit_no_change_closure(
    goal: &GoalContract,
    run: &AgentRun,
    ledger: &TaskLedger,
    receipt: &EditNoChangeReceipt,
    now: AgentRunTimestamp,
) -> Result<EditNoChangeClosure, EditNoChangeFailure> {
    let a = receipt.anchor();
    if run.state() != AgentControllerState::Execute
        || goal
            .original()
            .is_none_or(|original| original.digest() != receipt.original_goal())
        || !GoalOriginalCoverage::derive_from(goal)
            .map_err(|_| EditNoChangeFailure::Binding)?
            .is_complete()
        || goal.reference() != a.goal()
        || ledger.goal_contract() != a.goal()
        || run.goal_contract() != a.goal()
        || run.id() != a.run()
        || run.current_snapshot_id() != a.snapshot()
        || run.last_event_sequence() != a.sequence()
        || run.task_ledger_revision() != a.ledger()
        || ledger.revision() != a.ledger()
        || run.model_profile() != Some(receipt.profile())
        || ledger.pending_planning().is_some()
        || now < run.updated_at()
        || now < receipt.finished_at()
        || now >= receipt.deadline()
        || run
            .created_at()
            .unix_millis()
            .checked_add(run.budget().duration_limit().millis())
            != Some(receipt.deadline().unix_millis())
    {
        return Err(EditNoChangeFailure::Binding);
    }
    if run
        .budget_exhaustion(now)
        .map_err(|_| EditNoChangeFailure::Binding)?
        .is_some()
    {
        return Err(EditNoChangeFailure::BudgetExhausted);
    }
    let mut active = ledger
        .steps()
        .filter(|s| s.is_active_plan_step() && s.status() == TaskStepStatus::InProgress);
    let step = active.next().ok_or(EditNoChangeFailure::Binding)?;
    if active.next().is_some()
        || step.definition().id() != a.step()
        || step.definition().verification_spec().id() != a.verification()
        || !matches!(step.attempts().last(), Some(attempt) if attempt.run_id()==run.id()
            && attempt.number()==a.attempt() && matches!(attempt.outcome(), TaskStepAttemptOutcome::Active))
    {
        return Err(EditNoChangeFailure::Binding);
    }
    crate::AdmitSourceTaskStep
        .execute(goal, step.definition())
        .map_err(|_| EditNoChangeFailure::Binding)?;
    let mut next_ledger = ledger.clone();
    let mut next_run = run.clone();
    next_ledger.fail_step(a.step(), run.id(), TaskStepFailureReason::try_from_string(
        "Core rejected an unchanged current source replacement; the original proof remains open".to_owned()
    ).map_err(|_| EditNoChangeFailure::Binding)?, TaskLedgerTimestamp::from_unix_millis(now.unix_millis()).map_err(|_| EditNoChangeFailure::Binding)?)
        .map_err(EditNoChangeFailure::Ledger)?;
    let advance = AdvanceAgentController
        .execute(
            &mut next_run,
            AgentControllerSignal::ExecutionNeedsReplan,
            receipt.event_id(),
            a.snapshot(),
            now,
            false,
        )
        .map_err(EditNoChangeFailure::Controller)?;
    if next_run.state() != AgentControllerState::Replan {
        return Err(EditNoChangeFailure::BudgetExhausted);
    }
    Ok(EditNoChangeClosure {
        ledger: next_ledger,
        run: next_run,
        event: advance.into_event(),
    })
}

/// Restores and closes one real byte-identical edit without dispatching another
/// model, patch, source tool or operational verification.
#[derive(Clone, Copy)]
pub struct CloseEditNoChange<'a> {
    reservations: &'a dyn EditPhaseReservationStore,
    actions: &'a dyn AgentActionStore,
    patch: &'a dyn WorkspacePatchTool,
}
impl fmt::Debug for CloseEditNoChange<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloseEditNoChange").finish_non_exhaustive()
    }
}
impl<'a> CloseEditNoChange<'a> {
    /// Uses existing bounded reservation, atomic action and exact safe-base ports.
    pub const fn new(
        reservations: &'a dyn EditPhaseReservationStore,
        actions: &'a dyn AgentActionStore,
        patch: &'a dyn WorkspacePatchTool,
    ) -> Self {
        Self {
            reservations,
            actions,
            patch,
        }
    }

    /// Takes the ordinary current page again after cold reopen; it grants no new
    /// acquisition or delivery credit. The Store proves its original observed wire.
    // Keep the current authority, source, full immutable profile and CAS explicit.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute<C>(
        self,
        project: &ProjectIdentity,
        goal: &GoalContract,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        version: TaskLedgerStoreVersion,
        current: &PublishedIndex,
        profile: &ModelProfile,
        page: &AgentSourcePage,
        observed_at: AgentRunTimestamp,
        control: &C,
    ) -> Result<TaskLedgerStoreVersion, EditNoChangeFailure>
    where
        C: AgentControllerControl + WorkspacePatchControl,
    {
        let started = Instant::now();
        let checkpoint = |at| {
            if AgentControllerControl::is_cancelled(control) {
                return Err(EditNoChangeFailure::Cancelled);
            }
            if run
                .budget_exhaustion(at)
                .map_err(|_| EditNoChangeFailure::Binding)?
                .is_some()
            {
                return Err(EditNoChangeFailure::BudgetExhausted);
            }
            Ok(())
        };
        checkpoint(observed_at)?;
        if !profile.executable_actions_enabled() || run.model_profile() != Some(profile.reference())
        {
            return Err(EditNoChangeFailure::Binding);
        }
        let stored = StoredTaskLedger::new(ledger.clone(), version);
        let state = EditPhaseState::new(project, goal, &stored, run, current)
            .map_err(EditNoChangeFailure::Source)?;
        let phase = PreparedEditPhase::update(state, page).map_err(EditNoChangeFailure::Source)?;
        phase
            .revalidate(state, self.patch, control)
            .await
            .map_err(EditNoChangeFailure::Source)?;
        let now = crate::model_wire::phase_elapsed_timestamp(started, observed_at)
            .ok_or(EditNoChangeFailure::Clock)?;
        checkpoint(now)?;
        let receipt = self
            .reservations
            .load_edit_no_change(project, &phase, profile, current, now)
            .await
            .map_err(EditNoChangeFailure::Reservation)?
            .ok_or(EditNoChangeFailure::NoObservation)?;
        if receipt.anchor() != phase.anchor()
            || receipt.job_digest() != phase.job_digest()
            || receipt.window()
                != &phase
                    .no_change_window()
                    .ok_or(EditNoChangeFailure::Binding)?
            || receipt.index() != current.run().id()
            || receipt.profile() != profile.reference()
            || receipt.generation_limit() != profile.settings().output_limit().get()
            || receipt.context_limit() != profile.settings().context_limit().get()
        {
            return Err(EditNoChangeFailure::Binding);
        }
        phase
            .revalidate(state, self.patch, control)
            .await
            .map_err(EditNoChangeFailure::Source)?;
        let now = crate::model_wire::phase_elapsed_timestamp(started, observed_at)
            .ok_or(EditNoChangeFailure::Clock)?;
        checkpoint(now)?;
        let closure = derive_edit_no_change_closure(goal, run, ledger, &receipt, now)?;
        let next_version = self
            .actions
            .commit_edit_no_change_action(
                project,
                version,
                run.last_event_sequence(),
                current.run().id(),
                closure.ledger(),
                closure.run(),
                closure.event(),
                &receipt,
            )
            .await
            .map_err(EditNoChangeFailure::Storage)?;
        let (next_ledger, next_run, _) = closure.into_parts();
        *ledger = next_ledger;
        *run = next_run;
        Ok(next_version)
    }
}

/// Closed failures; no model text or error-display parsing controls the transition.
#[derive(Debug)]
pub enum EditNoChangeFailure {
    /// Any current Goal/Run/step/window/profile binding differed.
    Binding,
    /// No authenticated byte-identical Existing/Replace observation exists.
    NoObservation,
    /// Cancellation wins before the action CAS.
    Cancelled,
    /// Original bounded owner no longer has capacity to continue.
    BudgetExhausted,
    /// Actual elapsed time cannot be represented.
    Clock,
    /// Current full source revalidation failed.
    Source(EditPhaseFailure),
    /// Typed durable NoChange replay was denied.
    Reservation(EditPhaseReservationFailure),
    /// Atomic exact-index Ledger/Journal action was rejected.
    Storage(AgentActionStoreFailure),
    /// Existing TaskLedger transition was rejected.
    Ledger(TaskLedgerError),
    /// Existing controller transition was rejected.
    Controller(AgentControllerError),
}
impl fmt::Display for EditNoChangeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Binding => "NoChange closure bindings differ",
            Self::NoObservation => "No authenticated current NoChange exists",
            Self::Cancelled => "NoChange closure was cancelled",
            Self::BudgetExhausted => "NoChange original budget is exhausted",
            Self::Clock => "NoChange time is not representable",
            Self::Source(_) => "NoChange current source changed",
            Self::Reservation(_) => "NoChange receipt was rejected",
            Self::Storage(_) => "NoChange atomic closure was rejected",
            Self::Ledger(_) => "NoChange Ledger transition was rejected",
            Self::Controller(_) => "NoChange controller transition was rejected",
        })
    }
}
impl Error for EditNoChangeFailure {}
