use crate::{
    DeepMapModelDescriptor, DeepMapPhase, DeepMapPublicationAnchor, DeepMapSafeAction,
    DeepMapTargetKind,
};
use a3_domain::{
    DeepMapDiagnosticCode, DeepMapEventSequence, DeepMapMode, DeepMapRunId, DeepMapRunState,
    DeepMapRunTimestamp, ExploreCost, ExplorePlan, ExplorePlanStopReason, ExploreSeedReason,
    ModuleCardEvidenceId, ModuleCardField, ModuleId, ProjectIdentity, SymbolId,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Fixed maximum number of run summaries returned by one journal read.
pub const DEEP_MAP_RUN_PAGE_LIMIT: u16 = 20;
/// Fixed maximum number of events returned by one journal read.
pub const DEEP_MAP_ENTRY_PAGE_LIMIT: u16 = 50;
/// Fixed maximum number of module summaries returned by one dashboard read.
pub const DEEP_MAP_MODULE_PAGE_LIMIT: u16 = 20;
/// Fixed maximum number of resolved plan steps returned by one dashboard read.
pub const DEEP_MAP_MODULE_STEP_PAGE_LIMIT: u16 = 50;

/// Closed, content-free reference to the exact target selected by the planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapPlanTargetReference {
    /// A deterministic module projection.
    Module(ModuleId),
    /// An exact file revision represented only by its Evidence ID.
    FileEvidence(ModuleCardEvidenceId),
    /// An exact structural symbol.
    Symbol(SymbolId),
}

/// User-facing plan data retained without source content or model output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepMapPlanStep {
    position: u64,
    module_id: ModuleId,
    target_kind: DeepMapTargetKind,
    target_reference: Option<DeepMapPlanTargetReference>,
    seed_reason: ExploreSeedReason,
    coverage_fields: Option<Vec<ModuleCardField>>,
    confirmed: bool,
}

impl DeepMapPlanStep {
    /// Reconstructs one current or legacy plan step.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        position: u64,
        module_id: ModuleId,
        target_kind: DeepMapTargetKind,
        target_reference: Option<DeepMapPlanTargetReference>,
        seed_reason: ExploreSeedReason,
        coverage_fields: Option<Vec<ModuleCardField>>,
        confirmed: bool,
    ) -> Result<Self, DeepMapRunJournalFailure> {
        if position == 0
            || target_reference.is_some() != coverage_fields.is_some()
            || coverage_fields.as_ref().is_some_and(Vec::is_empty)
            || coverage_fields
                .as_ref()
                .is_some_and(|fields| fields.windows(2).any(|pair| pair[0] >= pair[1]))
            || target_reference.is_some_and(|reference| {
                !matches!(
                    (target_kind, reference),
                    (
                        DeepMapTargetKind::Module,
                        DeepMapPlanTargetReference::Module(_)
                    ) | (
                        DeepMapTargetKind::Manifest,
                        DeepMapPlanTargetReference::FileEvidence(_)
                    ) | (
                        DeepMapTargetKind::Symbol,
                        DeepMapPlanTargetReference::Symbol(_)
                    )
                )
            })
        {
            return Err(DeepMapRunJournalFailure::InvalidStoredData);
        }
        Ok(Self {
            position,
            module_id,
            target_kind,
            target_reference,
            seed_reason,
            coverage_fields,
            confirmed,
        })
    }

    /// Returns the one-based planner position.
    #[must_use]
    pub const fn position(&self) -> u64 {
        self.position
    }
    /// Returns the module whose card gains coverage.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }
    /// Returns the closed target class.
    #[must_use]
    pub const fn target_kind(&self) -> DeepMapTargetKind {
        self.target_kind
    }
    /// Returns the exact opaque target when V29 details are available.
    #[must_use]
    pub const fn target_reference(&self) -> Option<DeepMapPlanTargetReference> {
        self.target_reference
    }
    /// Returns why this exploration was planned.
    #[must_use]
    pub const fn seed_reason(&self) -> ExploreSeedReason {
        self.seed_reason
    }
    /// Returns canonical intended card fields, or `None` for a legacy run.
    #[must_use]
    pub fn coverage_fields(&self) -> Option<&[ModuleCardField]> {
        self.coverage_fields.as_deref()
    }
    /// Returns whether the step produced verified evidence.
    #[must_use]
    pub const fn confirmed(&self) -> bool {
        self.confirmed
    }
}

/// Stable keyset cursor for module summaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepMapModuleCursor(ModuleId);

impl DeepMapModuleCursor {
    /// Creates a cursor after one canonical module identity.
    #[must_use]
    pub const fn new(module_id: ModuleId) -> Self {
        Self(module_id)
    }
    /// Returns the exclusive module identity.
    #[must_use]
    pub const fn module_id(self) -> ModuleId {
        self.0
    }
}

/// Aggregate planner progress for one module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepMapRunModuleSummary {
    module_id: ModuleId,
    planned_steps: u64,
    confirmed_steps: u64,
}

impl DeepMapRunModuleSummary {
    /// Reconstructs validated per-module progress.
    pub fn new(
        module_id: ModuleId,
        planned_steps: u64,
        confirmed_steps: u64,
    ) -> Result<Self, DeepMapRunJournalFailure> {
        if planned_steps == 0 || confirmed_steps > planned_steps {
            return Err(DeepMapRunJournalFailure::InvalidStoredData);
        }
        Ok(Self {
            module_id,
            planned_steps,
            confirmed_steps,
        })
    }
    /// Returns the module identity.
    #[must_use]
    pub const fn module_id(self) -> ModuleId {
        self.module_id
    }
    /// Returns the number of planned explorations.
    #[must_use]
    pub const fn planned_steps(self) -> u64 {
        self.planned_steps
    }
    /// Returns the number of confirmed explorations.
    #[must_use]
    pub const fn confirmed_steps(self) -> u64 {
        self.confirmed_steps
    }
}

/// Bounded canonical page of modules represented in a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepMapRunModulePage {
    modules: Vec<DeepMapRunModuleSummary>,
    next_cursor: Option<DeepMapModuleCursor>,
}

impl DeepMapRunModulePage {
    /// Creates a page within the fixed dashboard bound.
    pub fn new(
        modules: Vec<DeepMapRunModuleSummary>,
        next_cursor: Option<DeepMapModuleCursor>,
    ) -> Result<Self, DeepMapRunJournalFailure> {
        if modules.len() > usize::from(DEEP_MAP_MODULE_PAGE_LIMIT) {
            return Err(DeepMapRunJournalFailure::InvalidStoredData);
        }
        Ok(Self {
            modules,
            next_cursor,
        })
    }
    /// Returns module summaries in canonical identity order.
    #[must_use]
    pub fn modules(&self) -> &[DeepMapRunModuleSummary] {
        &self.modules
    }
    /// Returns the next exclusive module cursor.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<DeepMapModuleCursor> {
        self.next_cursor
    }
}

/// Bounded planner-step page for one module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepMapModuleStepPage {
    steps: Vec<DeepMapPlanStep>,
    next_after_position: Option<u64>,
}

impl DeepMapModuleStepPage {
    /// Creates a canonical page within the fixed dashboard bound.
    pub fn new(
        steps: Vec<DeepMapPlanStep>,
        next_after_position: Option<u64>,
    ) -> Result<Self, DeepMapRunJournalFailure> {
        if steps.len() > usize::from(DEEP_MAP_MODULE_STEP_PAGE_LIMIT)
            || steps
                .windows(2)
                .any(|pair| pair[0].position() >= pair[1].position())
        {
            return Err(DeepMapRunJournalFailure::InvalidStoredData);
        }
        Ok(Self {
            steps,
            next_after_position,
        })
    }
    /// Returns steps in planner order.
    #[must_use]
    pub fn steps(&self) -> &[DeepMapPlanStep] {
        &self.steps
    }
    /// Returns the exclusive next position.
    #[must_use]
    pub const fn next_after_position(&self) -> Option<u64> {
        self.next_after_position
    }
}

/// Immutable metadata captured before a Deep-Map worker can start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepMapRunStart {
    id: DeepMapRunId,
    anchor: DeepMapPublicationAnchor,
    mode: DeepMapMode,
    model: DeepMapModelDescriptor,
    created_at: DeepMapRunTimestamp,
}

impl DeepMapRunStart {
    /// Captures a safe run envelope without endpoint, credential, or provider payload data.
    #[must_use]
    pub const fn new(
        id: DeepMapRunId,
        anchor: DeepMapPublicationAnchor,
        mode: DeepMapMode,
        model: DeepMapModelDescriptor,
        created_at: DeepMapRunTimestamp,
    ) -> Self {
        Self {
            id,
            anchor,
            mode,
            model,
            created_at,
        }
    }

    /// Returns the opaque run identity.
    #[must_use]
    pub const fn id(&self) -> DeepMapRunId {
        self.id
    }
    /// Returns the immutable Fast-Index anchor for this run.
    #[must_use]
    pub const fn anchor(&self) -> DeepMapPublicationAnchor {
        self.anchor
    }
    /// Returns the selected fixed-budget mode.
    #[must_use]
    pub const fn mode(&self) -> DeepMapMode {
        self.mode
    }
    /// Returns the safe provider/model descriptor captured at start.
    #[must_use]
    pub const fn model(&self) -> &DeepMapModelDescriptor {
        &self.model
    }
    /// Returns the local creation timestamp.
    #[must_use]
    pub const fn created_at(&self) -> DeepMapRunTimestamp {
        self.created_at
    }
}

/// Closed result attached to one safe journal event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapEventResult {
    /// Work has started but no terminal assertion is made.
    Pending,
    /// A deterministic exploration step was confirmed.
    Confirmed,
    /// The latest index was already completely mapped.
    AlreadyCurrent,
    /// Verified cards were committed atomically.
    Published,
    /// A checkpoint was retained for deliberate resume.
    Paused,
    /// A retained checkpoint was resumed.
    Resumed,
    /// The run was deliberately cancelled.
    Cancelled,
    /// The run ended with the attached safe diagnosis.
    Failed,
    /// A non-terminal run was reconciled after restart.
    Interrupted,
}

/// Safe closed result of the immutable Module-Card publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapPublicationResult {
    /// This run committed the verified cards.
    Published,
    /// The same index already had a complete publication.
    AlreadyCurrent,
}

/// Materialized technical metadata for one planner-produced step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepMapStepDetail {
    target_kind: DeepMapTargetKind,
    seed_reason: ExploreSeedReason,
    reserved_cost: ExploreCost,
    information_gain_basis_points: u16,
    coverage_field_count: u16,
    confirmed: bool,
}

impl DeepMapStepDetail {
    /// Creates validated content-free step metadata.
    pub fn new(
        target_kind: DeepMapTargetKind,
        seed_reason: ExploreSeedReason,
        reserved_cost: ExploreCost,
        information_gain_basis_points: u16,
        coverage_field_count: u16,
        confirmed: bool,
    ) -> Result<Self, DeepMapRunJournalFailure> {
        if information_gain_basis_points > 10_000 || coverage_field_count == 0 {
            return Err(DeepMapRunJournalFailure::InvalidStoredData);
        }
        Ok(Self {
            target_kind,
            seed_reason,
            reserved_cost,
            information_gain_basis_points,
            coverage_field_count,
            confirmed,
        })
    }

    /// Returns the closed target class without source content.
    #[must_use]
    pub const fn target_kind(self) -> DeepMapTargetKind {
        self.target_kind
    }

    /// Returns why the deterministic planner selected the step.
    #[must_use]
    pub const fn seed_reason(self) -> ExploreSeedReason {
        self.seed_reason
    }

    /// Returns the conservative cost reserved before execution.
    #[must_use]
    pub const fn reserved_cost(self) -> ExploreCost {
        self.reserved_cost
    }

    /// Returns the deterministic gain estimate in basis points.
    #[must_use]
    pub const fn information_gain_basis_points(self) -> u16 {
        self.information_gain_basis_points
    }

    /// Returns how many Module-Card fields the step was expected to cover.
    #[must_use]
    pub const fn coverage_field_count(self) -> u16 {
        self.coverage_field_count
    }

    /// Returns whether the evidence-backed step was confirmed.
    #[must_use]
    pub const fn confirmed(self) -> bool {
        self.confirmed
    }
}

/// One append-only, content-free Deep-Map pipeline event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepMapJournalEvent {
    sequence: DeepMapEventSequence,
    occurred_at: DeepMapRunTimestamp,
    state: DeepMapRunState,
    phase: Option<DeepMapPhase>,
    target_kind: Option<DeepMapTargetKind>,
    action: Option<DeepMapSafeAction>,
    module_id: Option<ModuleId>,
    step_position: Option<u64>,
    total_steps: Option<u64>,
    confirmed: bool,
    result: DeepMapEventResult,
    diagnostic: Option<DeepMapDiagnosticCode>,
}

impl DeepMapJournalEvent {
    /// Creates a safe event. Step pairs and failure diagnosis must remain internally consistent.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sequence: DeepMapEventSequence,
        occurred_at: DeepMapRunTimestamp,
        state: DeepMapRunState,
        phase: Option<DeepMapPhase>,
        target_kind: Option<DeepMapTargetKind>,
        action: Option<DeepMapSafeAction>,
        module_id: Option<ModuleId>,
        step_position: Option<u64>,
        total_steps: Option<u64>,
        confirmed: bool,
        result: DeepMapEventResult,
        diagnostic: Option<DeepMapDiagnosticCode>,
    ) -> Result<Self, DeepMapRunJournalFailure> {
        if step_position.is_some() != total_steps.is_some()
            || step_position == Some(0)
            || total_steps == Some(0)
            || step_position
                .zip(total_steps)
                .is_some_and(|(step, total)| step > total)
            || (state == DeepMapRunState::Failed) != diagnostic.is_some()
            || (result == DeepMapEventResult::Failed) != diagnostic.is_some()
        {
            return Err(DeepMapRunJournalFailure::InvalidInput);
        }
        Ok(Self {
            sequence,
            occurred_at,
            state,
            phase,
            target_kind,
            action,
            module_id,
            step_position,
            total_steps,
            confirmed,
            result,
            diagnostic,
        })
    }

    /// Returns the monotone per-run event sequence.
    #[must_use]
    pub const fn sequence(self) -> DeepMapEventSequence {
        self.sequence
    }
    /// Returns when the event occurred.
    #[must_use]
    pub const fn occurred_at(self) -> DeepMapRunTimestamp {
        self.occurred_at
    }
    /// Returns the materialized run state after this event.
    #[must_use]
    pub const fn state(self) -> DeepMapRunState {
        self.state
    }
    /// Returns the safe pipeline phase, when applicable.
    #[must_use]
    pub const fn phase(self) -> Option<DeepMapPhase> {
        self.phase
    }
    /// Returns the closed target class, when applicable.
    #[must_use]
    pub const fn target_kind(self) -> Option<DeepMapTargetKind> {
        self.target_kind
    }
    /// Returns the closed action class, when applicable.
    #[must_use]
    pub const fn action(self) -> Option<DeepMapSafeAction> {
        self.action
    }
    /// Returns the opaque module identity, when the event targets one module.
    #[must_use]
    pub const fn module_id(self) -> Option<ModuleId> {
        self.module_id
    }
    /// Returns the one-based deterministic step position, when applicable.
    #[must_use]
    pub const fn step_position(self) -> Option<u64> {
        self.step_position
    }
    /// Returns the total deterministic plan steps, when applicable.
    #[must_use]
    pub const fn total_steps(self) -> Option<u64> {
        self.total_steps
    }
    /// Returns whether the event represents confirmed evidence-backed work.
    #[must_use]
    pub const fn confirmed(self) -> bool {
        self.confirmed
    }
    /// Returns the closed event result.
    #[must_use]
    pub const fn result(self) -> DeepMapEventResult {
        self.result
    }
    /// Returns the safe diagnosis for a failed event.
    #[must_use]
    pub const fn diagnostic(self) -> Option<DeepMapDiagnosticCode> {
        self.diagnostic
    }
}

/// Latest safe materialized state of one durable run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepMapRunSummary {
    start: DeepMapRunStart,
    state: DeepMapRunState,
    updated_at: DeepMapRunTimestamp,
    confirmed_steps: u64,
    total_steps: u64,
    diagnostic: Option<DeepMapDiagnosticCode>,
    details_incomplete: bool,
    latest_sequence: DeepMapEventSequence,
    plan_stop_reason: Option<ExplorePlanStopReason>,
    publication_result: Option<DeepMapPublicationResult>,
}

impl DeepMapRunSummary {
    /// Reconstructs a validated materialized run projection from durable data.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        start: DeepMapRunStart,
        state: DeepMapRunState,
        updated_at: DeepMapRunTimestamp,
        confirmed_steps: u64,
        total_steps: u64,
        diagnostic: Option<DeepMapDiagnosticCode>,
        details_incomplete: bool,
        latest_sequence: DeepMapEventSequence,
        plan_stop_reason: Option<ExplorePlanStopReason>,
        publication_result: Option<DeepMapPublicationResult>,
    ) -> Result<Self, DeepMapRunJournalFailure> {
        if confirmed_steps > total_steps
            || (state == DeepMapRunState::Failed) != diagnostic.is_some()
            || updated_at < start.created_at()
        {
            return Err(DeepMapRunJournalFailure::InvalidStoredData);
        }
        Ok(Self {
            start,
            state,
            updated_at,
            confirmed_steps,
            total_steps,
            diagnostic,
            details_incomplete,
            latest_sequence,
            plan_stop_reason,
            publication_result,
        })
    }

    /// Returns the immutable start envelope.
    #[must_use]
    pub const fn start(&self) -> &DeepMapRunStart {
        &self.start
    }
    /// Returns the latest run state.
    #[must_use]
    pub const fn state(&self) -> DeepMapRunState {
        self.state
    }
    /// Returns the timestamp of the latest durable update.
    #[must_use]
    pub const fn updated_at(&self) -> DeepMapRunTimestamp {
        self.updated_at
    }
    /// Returns the number of confirmed planner steps.
    #[must_use]
    pub const fn confirmed_steps(&self) -> u64 {
        self.confirmed_steps
    }
    /// Returns the number of materialized planner steps.
    #[must_use]
    pub const fn total_steps(&self) -> u64 {
        self.total_steps
    }
    /// Returns the closed failure diagnosis, if the run failed.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<DeepMapDiagnosticCode> {
        self.diagnostic
    }
    /// Returns whether non-critical journal writes were lost.
    #[must_use]
    pub const fn details_incomplete(&self) -> bool {
        self.details_incomplete
    }
    /// Returns the latest persisted event sequence.
    #[must_use]
    pub const fn latest_sequence(&self) -> DeepMapEventSequence {
        self.latest_sequence
    }

    /// Returns why deterministic planning stopped, when a plan was materialized.
    #[must_use]
    pub const fn plan_stop_reason(&self) -> Option<ExplorePlanStopReason> {
        self.plan_stop_reason
    }

    /// Returns the closed publication outcome, when publication was reached.
    #[must_use]
    pub const fn publication_result(&self) -> Option<DeepMapPublicationResult> {
        self.publication_result
    }
}

/// One exact event plus its safe run and optional planner-step metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepMapEntryDetail {
    run: DeepMapRunSummary,
    event: DeepMapJournalEvent,
    step: Option<DeepMapStepDetail>,
}

impl DeepMapEntryDetail {
    /// Bundles an event with project-validated durable metadata.
    #[must_use]
    pub const fn new(
        run: DeepMapRunSummary,
        event: DeepMapJournalEvent,
        step: Option<DeepMapStepDetail>,
    ) -> Self {
        Self { run, event, step }
    }

    /// Returns the materialized run projection.
    #[must_use]
    pub const fn run(&self) -> &DeepMapRunSummary {
        &self.run
    }

    /// Returns the selected safe event.
    #[must_use]
    pub const fn event(&self) -> DeepMapJournalEvent {
        self.event
    }

    /// Returns planner metadata when this event names a numbered step.
    #[must_use]
    pub const fn step(&self) -> Option<DeepMapStepDetail> {
        self.step
    }
}

/// Stable keyset cursor for older run summaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeepMapRunCursor {
    updated_at: DeepMapRunTimestamp,
    run_id: DeepMapRunId,
}

impl DeepMapRunCursor {
    /// Creates a stable keyset cursor for the next older page.
    #[must_use]
    pub const fn new(updated_at: DeepMapRunTimestamp, run_id: DeepMapRunId) -> Self {
        Self { updated_at, run_id }
    }
    /// Returns the timestamp component of the keyset cursor.
    #[must_use]
    pub const fn updated_at(self) -> DeepMapRunTimestamp {
        self.updated_at
    }
    /// Returns the run identity component of the keyset cursor.
    #[must_use]
    pub const fn run_id(self) -> DeepMapRunId {
        self.run_id
    }
}

/// Bounded newest-first page of durable run summaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepMapRunPage {
    runs: Vec<DeepMapRunSummary>,
    next_cursor: Option<DeepMapRunCursor>,
}

impl DeepMapRunPage {
    /// Creates a page that cannot exceed the fixed run read bound.
    pub fn new(
        runs: Vec<DeepMapRunSummary>,
        next_cursor: Option<DeepMapRunCursor>,
    ) -> Result<Self, DeepMapRunJournalFailure> {
        if runs.len() > usize::from(DEEP_MAP_RUN_PAGE_LIMIT) {
            return Err(DeepMapRunJournalFailure::InvalidStoredData);
        }
        Ok(Self { runs, next_cursor })
    }
    /// Returns the newest-first summaries.
    #[must_use]
    pub fn runs(&self) -> &[DeepMapRunSummary] {
        &self.runs
    }
    /// Returns the keyset cursor for the next older page.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<DeepMapRunCursor> {
        self.next_cursor
    }
}

/// Bounded chronological page of safe journal events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepMapEntryPage {
    entries: Vec<DeepMapJournalEvent>,
    next_before_sequence: Option<DeepMapEventSequence>,
}

impl DeepMapEntryPage {
    /// Creates a chronological page that cannot exceed the fixed event read bound.
    pub fn new(
        entries: Vec<DeepMapJournalEvent>,
        next_before_sequence: Option<DeepMapEventSequence>,
    ) -> Result<Self, DeepMapRunJournalFailure> {
        if entries.len() > usize::from(DEEP_MAP_ENTRY_PAGE_LIMIT)
            || entries
                .windows(2)
                .any(|pair| pair[0].sequence() >= pair[1].sequence())
        {
            return Err(DeepMapRunJournalFailure::InvalidStoredData);
        }
        Ok(Self {
            entries,
            next_before_sequence,
        })
    }
    /// Returns chronological events for rendering.
    #[must_use]
    pub fn entries(&self) -> &[DeepMapJournalEvent] {
        &self.entries
    }
    /// Returns the exclusive sequence cursor for the next older page.
    #[must_use]
    pub const fn next_before_sequence(&self) -> Option<DeepMapEventSequence> {
        self.next_before_sequence
    }
}

/// Owned asynchronous result returned by the durable journal boundary.
pub type DeepMapRunJournalFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, DeepMapRunJournalFailure>> + Send + 'a>>;

/// Durable project-bound Deep-Map run journal boundary.
pub trait DeepMapRunJournalStore: fmt::Debug + Send + Sync {
    /// Creates the immutable run envelope before the worker starts.
    fn create_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run: &'a DeepMapRunStart,
    ) -> DeepMapRunJournalFuture<'a, ()>;
    /// Materializes every deterministic planner step transactionally.
    fn record_plan<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: DeepMapRunId,
        plan: &'a ExplorePlan,
    ) -> DeepMapRunJournalFuture<'a, ()>;
    /// Appends one event and updates the run projection in the same transaction.
    fn append_event<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: DeepMapRunId,
        event: DeepMapJournalEvent,
    ) -> DeepMapRunJournalFuture<'a, ()>;
    /// Marks non-critical journal loss without changing the worker outcome.
    fn mark_details_incomplete<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: DeepMapRunId,
    ) -> DeepMapRunJournalFuture<'a, ()>;
    /// Converts non-terminal runs left by a prior process into interrupted runs.
    fn reconcile_interrupted<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        occurred_at: DeepMapRunTimestamp,
    ) -> DeepMapRunJournalFuture<'a, u64>;
    /// Loads one project-bound newest-first run page.
    fn list_runs<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        cursor: Option<DeepMapRunCursor>,
    ) -> DeepMapRunJournalFuture<'a, DeepMapRunPage>;
    /// Loads one exact project-bound run summary.
    fn load_run<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: DeepMapRunId,
    ) -> DeepMapRunJournalFuture<'a, Option<DeepMapRunSummary>>;
    /// Loads one bounded canonical page of modules represented in the plan.
    fn list_run_modules<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: DeepMapRunId,
        cursor: Option<DeepMapModuleCursor>,
    ) -> DeepMapRunJournalFuture<'a, DeepMapRunModulePage>;
    /// Loads one bounded page of safe plan details for a module.
    fn list_module_steps<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: DeepMapRunId,
        module_id: ModuleId,
        after_position: Option<u64>,
    ) -> DeepMapRunJournalFuture<'a, DeepMapModuleStepPage>;
    /// Loads one project-bound chronological event page.
    fn list_entries<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: DeepMapRunId,
        before_sequence: Option<DeepMapEventSequence>,
    ) -> DeepMapRunJournalFuture<'a, DeepMapEntryPage>;
    /// Loads one exact event with safe run and planner-step metadata.
    fn load_entry<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: DeepMapRunId,
        sequence: DeepMapEventSequence,
    ) -> DeepMapRunJournalFuture<'a, Option<DeepMapEntryDetail>>;
}

/// Stable content-free journal failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepMapRunJournalFailure {
    /// Caller supplied an invalid run, cursor, or event transition.
    InvalidInput,
    /// Durable rows contradicted the journal schema or domain invariants.
    InvalidStoredData,
    /// A monotone event sequence or immutable run envelope conflicted.
    Conflict,
    /// Local journal storage could not be read or written.
    Unavailable,
}

impl fmt::Display for DeepMapRunJournalFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInput => "Deep-Map journal request is invalid",
            Self::InvalidStoredData => "Deep-Map journal contains invalid data",
            Self::Conflict => "Deep-Map journal sequence changed",
            Self::Unavailable => "Deep-Map journal is unavailable",
        })
    }
}

impl Error for DeepMapRunJournalFailure {}
