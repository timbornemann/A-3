use crate::{
    ContextToolResultDigest, IndexPersistenceControl, KnowledgeIndexFailure, KnowledgeIndexStore,
    RunJournalStore, RunJournalStoreFailure, StoredTaskLedger, TaskLedgerStore,
    TaskLedgerStoreFailure, TaskLedgerStoreVersion,
};
use a3_domain::{
    AgentControllerState, AgentMutationAttempt, AgentMutationDisposition, AgentMutationKind,
    AgentRun, AgentRunError, AgentRunId, AgentRunTimestamp, AgentToolAttempt,
    AgentToolAttemptNumber, AgentToolAttemptStatus, AgentToolEvidence, MutationActionFingerprint,
    ProjectIdentity, RunEvent, RunEventCode, RunEventId, RunEventKind, RunEventOutcome,
    RunEventPayload, RunEventSequence, SnapshotId, TaskEvidenceId, TaskLedger, TaskLedgerError,
    TaskLedgerTimestamp, TaskStepCancellationReason, TaskStepId, TaskStepStatus, ToolRunId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

const MAX_RECOVERY_EVIDENCE: usize = 16_384;
const INVALIDATION_BATCH_SIZE: usize = 64;

/// Owned future returned by the object-safe durable recovery capability.
pub type AgentRecoveryStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, AgentRecoveryStoreFailure>> + Send + 'a>>;

/// Content-free normalized mutation result retained with its definitive tool event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentMutationResultRecord {
    digest: ContextToolResultDigest,
    truncated: bool,
    observed_output_bytes: u64,
}

impl AgentMutationResultRecord {
    /// Creates the bounded result metadata produced by the owning mutation adapter.
    #[must_use]
    pub const fn new(
        digest: ContextToolResultDigest,
        truncated: bool,
        observed_output_bytes: u64,
    ) -> Self {
        Self {
            digest,
            truncated,
            observed_output_bytes,
        }
    }

    /// Returns the digest of the complete normalized result.
    #[must_use]
    pub const fn digest(self) -> ContextToolResultDigest {
        self.digest
    }

    /// Returns whether the retained result crossed a bounded output limit.
    #[must_use]
    pub const fn truncated(self) -> bool {
        self.truncated
    }

    /// Returns the normalized bytes observed before redaction or truncation.
    #[must_use]
    pub const fn observed_output_bytes(self) -> u64 {
        self.observed_output_bytes
    }
}

/// Storage operations that must be atomic at the crash/restart boundary.
pub trait AgentRecoveryStore: fmt::Debug + Send + Sync {
    /// Persists a new attempt before invoking the corresponding tool capability.
    fn begin_agent_tool_attempt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
        snapshot_id: SnapshotId,
        tool_run_id: ToolRunId,
        started_at: AgentRunTimestamp,
    ) -> AgentRecoveryStoreFuture<'a, AgentToolAttempt>;

    /// Atomically starts a mutation as Unknown only when no prior Unknown blocks the worktree.
    #[allow(clippy::too_many_arguments)]
    fn begin_agent_mutation_attempt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
        snapshot_id: SnapshotId,
        tool_run_id: ToolRunId,
        fingerprint: MutationActionFingerprint,
        kind: AgentMutationKind,
        started_at: AgentRunTimestamp,
    ) -> AgentRecoveryStoreFuture<'a, AgentMutationAttempt>;

    /// Terminates an attempt that failed before a normalized result could be journaled.
    fn finish_agent_tool_attempt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        tool_run_id: ToolRunId,
        attempt: AgentToolAttemptNumber,
        status: AgentToolAttemptStatus,
        finished_at: AgentRunTimestamp,
    ) -> AgentRecoveryStoreFuture<'a, AgentToolAttempt>;

    /// Atomically closes a non-successful mutation lifecycle with its observed disposition.
    fn finish_agent_mutation_attempt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        tool_run_id: ToolRunId,
        attempt: AgentToolAttemptNumber,
        status: AgentToolAttemptStatus,
        disposition: AgentMutationDisposition,
        finished_at: AgentRunTimestamp,
    ) -> AgentRecoveryStoreFuture<'a, AgentMutationAttempt>;

    /// Atomically journals one normalized successful tool event and closes its in-flight attempt.
    #[allow(clippy::too_many_arguments)]
    fn complete_agent_tool_attempt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        event: &'a RunEvent,
        tool_run_id: ToolRunId,
        attempt: AgentToolAttemptNumber,
    ) -> AgentRecoveryStoreFuture<'a, AgentToolAttempt>;

    /// Atomically journals success and changes the matching mutation from Unknown to Applied.
    #[allow(clippy::too_many_arguments)]
    fn complete_agent_mutation_attempt<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        event: &'a RunEvent,
        tool_run_id: ToolRunId,
        attempt: AgentToolAttemptNumber,
        result: AgentMutationResultRecord,
    ) -> AgentRecoveryStoreFuture<'a, AgentMutationAttempt>;

    /// Marks every attempt left in flight by an application stop as interrupted.
    fn interrupt_agent_tool_attempts<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
        interrupted_at: AgentRunTimestamp,
    ) -> AgentRecoveryStoreFuture<'a, u32>;

    /// Loads the bounded content-free mutation history for one run in stable attempt order.
    fn load_agent_mutation_attempts<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
    ) -> AgentRecoveryStoreFuture<'a, Vec<AgentMutationAttempt>>;

    /// Atomically adopts a full published snapshot for exactly one Unknown mutation.
    #[allow(clippy::too_many_arguments)]
    fn reconcile_agent_mutation<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_last_sequence: RunEventSequence,
        run: &'a AgentRun,
        event: &'a RunEvent,
        tool_run_id: ToolRunId,
        attempt: AgentToolAttemptNumber,
    ) -> AgentRecoveryStoreFuture<'a, AgentMutationAttempt>;

    /// Loads only requested content-addressed tool evidence for one run.
    fn load_agent_tool_evidence<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        run_id: AgentRunId,
        evidence_ids: &'a [TaskEvidenceId],
    ) -> AgentRecoveryStoreFuture<'a, Vec<AgentToolEvidence>>;

    /// Atomically checks the current published snapshot and compare-and-swaps Ledger plus Run.
    #[allow(clippy::too_many_arguments)]
    fn commit_agent_recovery<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        choice: AgentRecoveryChoice,
        expected_published_snapshot: SnapshotId,
        expected_ledger_version: TaskLedgerStoreVersion,
        expected_last_sequence: RunEventSequence,
        ledger: &'a TaskLedger,
        run: &'a AgentRun,
        event: &'a RunEvent,
    ) -> AgentRecoveryStoreFuture<'a, TaskLedgerStoreVersion>;
}

/// Stable classification of recovery storage failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRecoveryStoreFailure {
    /// Worktree-local persistence was unavailable.
    Unavailable,
    /// Worktree-local persistence failed integrity checks.
    Corrupt,
    /// The database schema is newer than this application build.
    UnsupportedSchema,
    /// Durable content violated a relational or domain invariant.
    InvalidStoredData,
    /// The requested run was absent from this worktree.
    RunNotFound,
    /// The requested tool attempt was absent or already terminal.
    ToolAttemptConflict,
    /// An earlier Unknown mutation still requires authoritative reconciliation.
    MutationReconciliationRequired,
    /// Another writer advanced the run journal.
    RunSequenceConflict,
    /// Another writer advanced the Task Ledger projection.
    LedgerVersionConflict,
    /// Another index was published after recovery inspected repository state.
    PublishedSnapshotConflict,
    /// A fixed recovery resource boundary was exceeded.
    ResourceLimit,
}

impl fmt::Display for AgentRecoveryStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "agent recovery storage is unavailable",
            Self::Corrupt => "agent recovery storage is corrupt",
            Self::UnsupportedSchema => "agent recovery storage uses an unsupported schema",
            Self::InvalidStoredData => "agent recovery storage contains invalid data",
            Self::RunNotFound => "agent recovery run was not found",
            Self::ToolAttemptConflict => "agent tool-attempt lifecycle conflicts with storage",
            Self::MutationReconciliationRequired => {
                "an unknown mutation requires reconciliation before further mutation"
            }
            Self::RunSequenceConflict => "agent recovery run sequence changed concurrently",
            Self::LedgerVersionConflict => "agent recovery Ledger version changed concurrently",
            Self::PublishedSnapshotConflict => {
                "the published repository snapshot changed during agent recovery"
            }
            Self::ResourceLimit => "agent recovery exceeded a fixed resource limit",
        })
    }
}

impl Error for AgentRecoveryStoreFailure {}

/// Explicit user decision offered for one safely inspected non-terminal run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRecoveryChoice {
    /// Continue only after adopting the current published snapshot with fresh evidence.
    Resume,
    /// Reopen invalidated work and require a new plan before further model work.
    Replan,
    /// Terminate the current run without further model or tool work.
    Cancel,
}

/// Read-only, content-free recovery summary suitable for a later UI projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRecoveryInspection {
    run_id: AgentRunId,
    state: AgentControllerState,
    run_snapshot_id: SnapshotId,
    published_snapshot_id: SnapshotId,
    interrupted_tool_attempts: u32,
    stale_evidence_ids: Vec<TaskEvidenceId>,
    mutation_attempts: Vec<AgentMutationAttempt>,
}

impl AgentRecoveryInspection {
    /// Returns the inspected durable run identity.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the reconstructed finite controller state.
    #[must_use]
    pub const fn state(&self) -> AgentControllerState {
        self.state
    }

    /// Returns the snapshot held by the run when the application stopped.
    #[must_use]
    pub const fn run_snapshot_id(&self) -> SnapshotId {
        self.run_snapshot_id
    }

    /// Returns the latest atomically published snapshot checked during inspection.
    #[must_use]
    pub const fn published_snapshot_id(&self) -> SnapshotId {
        self.published_snapshot_id
    }

    /// Returns whether repository state advanced while the application was stopped.
    #[must_use]
    pub fn snapshot_changed(&self) -> bool {
        self.run_snapshot_id != self.published_snapshot_id
    }

    /// Returns how many in-flight attempts were durably marked interrupted.
    #[must_use]
    pub const fn interrupted_tool_attempts(&self) -> u32 {
        self.interrupted_tool_attempts
    }

    /// Returns verification evidence that cannot be resolved in the published index.
    #[must_use]
    pub fn stale_evidence_ids(&self) -> &[TaskEvidenceId] {
        &self.stale_evidence_ids
    }

    /// Returns every content-free mutation disposition in stable attempt order.
    #[must_use]
    pub fn mutation_attempts(&self) -> &[AgentMutationAttempt] {
        &self.mutation_attempts
    }

    /// Returns whether at least one Unknown still needs an authoritative full-scan baseline.
    #[must_use]
    pub fn mutation_reconciliation_required(&self) -> bool {
        self.mutation_attempts
            .iter()
            .any(|attempt| attempt.disposition().requires_reconciliation())
    }

    /// Returns whether an Unknown baseline exists but has not yet passed recovery Replan.
    #[must_use]
    pub fn mutation_replan_required(&self) -> bool {
        self.mutation_attempts
            .iter()
            .any(|attempt| attempt.disposition().requires_replan())
    }

    /// Resume is safe only while every completed verification remains evidence-current.
    #[must_use]
    pub fn can_resume(&self) -> bool {
        self.stale_evidence_ids.is_empty()
            && self
                .mutation_attempts
                .iter()
                .all(|attempt| attempt.disposition().permits_future_mutation())
    }
}

/// Result of atomically applying one explicit recovery choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRecoveryOutcome {
    kind: AgentRecoveryOutcomeKind,
    run: AgentRun,
    ledger: StoredTaskLedger,
    reopened_step_ids: Vec<TaskStepId>,
    interrupted_tool_attempts: u32,
}

impl AgentRecoveryOutcome {
    /// Returns the applied user-visible disposition.
    #[must_use]
    pub const fn kind(&self) -> AgentRecoveryOutcomeKind {
        self.kind
    }

    /// Returns the resulting materialized run.
    #[must_use]
    pub const fn run(&self) -> &AgentRun {
        &self.run
    }

    /// Returns the resulting durable Task Ledger and new CAS version.
    #[must_use]
    pub const fn ledger(&self) -> &StoredTaskLedger {
        &self.ledger
    }

    /// Returns completed steps reopened because their verification became stale.
    #[must_use]
    pub fn reopened_step_ids(&self) -> &[TaskStepId] {
        &self.reopened_step_ids
    }

    /// Returns how many abandoned tool attempts this application start interrupted.
    #[must_use]
    pub const fn interrupted_tool_attempts(&self) -> u32 {
        self.interrupted_tool_attempts
    }
}

/// Stable result classification after a recovery choice is committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRecoveryOutcomeKind {
    /// The run adopted the latest published snapshot and may continue.
    Resumed,
    /// Invalid work was reopened and the caller must enter the existing replan path.
    ReplanRequired,
    /// The run reached the terminal Cancelled state.
    Cancelled,
}

#[derive(Debug)]
struct RecoveryMaterial {
    run: AgentRun,
    stored_ledger: StoredTaskLedger,
    published_snapshot_id: SnapshotId,
    stale_evidence_ids: Vec<TaskEvidenceId>,
    mutation_attempts: Vec<AgentMutationAttempt>,
    interrupted_tool_attempts: u32,
}

/// Loads authoritative run state and marks abandoned tool attempts before offering choices.
#[derive(Debug, Clone, Copy)]
pub struct InspectAgentRunRecovery<'a> {
    recovery: &'a dyn AgentRecoveryStore,
    journal: &'a dyn RunJournalStore,
    ledgers: &'a dyn TaskLedgerStore,
    index: &'a dyn KnowledgeIndexStore,
}

impl<'a> InspectAgentRunRecovery<'a> {
    /// Creates the use case from its four narrow outward capabilities.
    #[must_use]
    pub const fn new(
        recovery: &'a dyn AgentRecoveryStore,
        journal: &'a dyn RunJournalStore,
        ledgers: &'a dyn TaskLedgerStore,
        index: &'a dyn KnowledgeIndexStore,
    ) -> Self {
        Self {
            recovery,
            journal,
            ledgers,
            index,
        }
    }

    /// Reconstructs the latest state and evaluates verification evidence against the index.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        run_id: AgentRunId,
        observed_at: AgentRunTimestamp,
        control: &dyn IndexPersistenceControl,
    ) -> Result<AgentRecoveryInspection, AgentRecoveryError> {
        let material = load_recovery_material(
            self.recovery,
            self.journal,
            self.ledgers,
            self.index,
            project,
            run_id,
            observed_at,
            control,
        )
        .await?;
        Ok(AgentRecoveryInspection {
            run_id,
            state: material.run.state(),
            run_snapshot_id: material.run.current_snapshot_id(),
            published_snapshot_id: material.published_snapshot_id,
            interrupted_tool_attempts: material.interrupted_tool_attempts,
            stale_evidence_ids: material.stale_evidence_ids,
            mutation_attempts: material.mutation_attempts,
        })
    }
}

/// Revalidates and atomically applies one explicit Resume, Replan, or Cancel decision.
#[derive(Debug, Clone, Copy)]
pub struct RecoverAgentRun<'a> {
    recovery: &'a dyn AgentRecoveryStore,
    journal: &'a dyn RunJournalStore,
    ledgers: &'a dyn TaskLedgerStore,
    index: &'a dyn KnowledgeIndexStore,
}

impl<'a> RecoverAgentRun<'a> {
    /// Creates the use case from the same capabilities as inspection.
    #[must_use]
    pub const fn new(
        recovery: &'a dyn AgentRecoveryStore,
        journal: &'a dyn RunJournalStore,
        ledgers: &'a dyn TaskLedgerStore,
        index: &'a dyn KnowledgeIndexStore,
    ) -> Self {
        Self {
            recovery,
            journal,
            ledgers,
            index,
        }
    }

    /// Rechecks all anchors immediately before the single transactional recovery commit.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        run_id: AgentRunId,
        choice: AgentRecoveryChoice,
        event_id: RunEventId,
        observed_at: AgentRunTimestamp,
        control: &dyn IndexPersistenceControl,
    ) -> Result<AgentRecoveryOutcome, AgentRecoveryError> {
        let material = load_recovery_material(
            self.recovery,
            self.journal,
            self.ledgers,
            self.index,
            project,
            run_id,
            observed_at,
            control,
        )
        .await?;
        let reconciliation_required = material
            .mutation_attempts
            .iter()
            .any(|attempt| attempt.disposition().requires_reconciliation());
        let mutation_replan_required = material
            .mutation_attempts
            .iter()
            .any(|attempt| attempt.disposition().requires_replan());
        if choice != AgentRecoveryChoice::Cancel && reconciliation_required {
            return Err(AgentRecoveryError::MutationReconciliationRequired);
        }
        if choice == AgentRecoveryChoice::Resume
            && (!material.stale_evidence_ids.is_empty() || mutation_replan_required)
        {
            return Err(AgentRecoveryError::ResumeRequiresReplan);
        }

        let expected_sequence = material.run.last_event_sequence();
        let expected_ledger_version = material.stored_ledger.version();
        let (mut ledger, _) = material.stored_ledger.into_parts();
        let mut run = material.run;
        let timestamp = TaskLedgerTimestamp::from_unix_millis(observed_at.unix_millis())
            .map_err(|_| AgentRecoveryError::InvalidTimestamp)?;
        let mut reopened = Vec::new();

        if matches!(
            choice,
            AgentRecoveryChoice::Replan | AgentRecoveryChoice::Cancel
        ) {
            reopened =
                reopen_invalidated_steps(&mut ledger, &material.stale_evidence_ids, timestamp)?;
            cancel_active_step_for_recovery(&mut ledger, choice, timestamp)?;
        }

        let (kind, event) = match choice {
            AgentRecoveryChoice::Resume => (
                AgentRecoveryOutcomeKind::Resumed,
                run.record(
                    event_id,
                    RunEventKind::Diagnostic,
                    RunEventPayload::new(
                        RunEventCode::StateRecovered,
                        Some(RunEventOutcome::Succeeded),
                        None,
                    ),
                    material.published_snapshot_id,
                    None,
                    observed_at,
                )?,
            ),
            AgentRecoveryChoice::Replan => (
                AgentRecoveryOutcomeKind::ReplanRequired,
                run.record(
                    event_id,
                    RunEventKind::Diagnostic,
                    RunEventPayload::new(
                        RunEventCode::StateRecovered,
                        Some(RunEventOutcome::Succeeded),
                        None,
                    ),
                    material.published_snapshot_id,
                    None,
                    observed_at,
                )?,
            ),
            AgentRecoveryChoice::Cancel => (
                AgentRecoveryOutcomeKind::Cancelled,
                run.transition(
                    event_id,
                    AgentControllerState::Cancelled,
                    RunEventPayload::new(
                        RunEventCode::UserRequest,
                        Some(RunEventOutcome::Cancelled),
                        None,
                    ),
                    material.published_snapshot_id,
                    observed_at,
                )?,
            ),
        };

        let ledger_version = self
            .recovery
            .commit_agent_recovery(
                project,
                choice,
                material.published_snapshot_id,
                expected_ledger_version,
                expected_sequence,
                &ledger,
                &run,
                &event,
            )
            .await?;
        Ok(AgentRecoveryOutcome {
            kind,
            run,
            ledger: StoredTaskLedger::new(ledger, ledger_version),
            reopened_step_ids: reopened,
            interrupted_tool_attempts: material.interrupted_tool_attempts,
        })
    }
}

#[allow(clippy::too_many_arguments)]
async fn load_recovery_material(
    recovery: &dyn AgentRecoveryStore,
    journal: &dyn RunJournalStore,
    ledgers: &dyn TaskLedgerStore,
    index: &dyn KnowledgeIndexStore,
    project: &ProjectIdentity,
    run_id: AgentRunId,
    observed_at: AgentRunTimestamp,
    control: &dyn IndexPersistenceControl,
) -> Result<RecoveryMaterial, AgentRecoveryError> {
    let interrupted_tool_attempts = recovery
        .interrupt_agent_tool_attempts(project, run_id, observed_at)
        .await?;
    let run = journal
        .load_agent_run(project, run_id)
        .await?
        .ok_or(AgentRecoveryError::RunNotFound)?;
    if run.state().is_terminal() {
        return Err(AgentRecoveryError::TerminalRun);
    }
    let stored_ledger = ledgers
        .load_task_ledger(project, run.goal_contract().task_id())
        .await?
        .ok_or(AgentRecoveryError::LedgerNotFound)?;
    if stored_ledger.ledger().goal_contract() != run.goal_contract()
        || stored_ledger.ledger().revision() != run.task_ledger_revision()
    {
        return Err(AgentRecoveryError::AnchorMismatch);
    }
    let mutation_attempts = recovery
        .load_agent_mutation_attempts(project, run_id)
        .await?;
    let published = index
        .latest_published_index(project, control)
        .await?
        .ok_or(AgentRecoveryError::PublishedIndexUnavailable)?;
    let evidence_ids = completed_verification_evidence(stored_ledger.ledger())?;
    let resolved = if evidence_ids.is_empty() {
        Vec::new()
    } else {
        recovery
            .load_agent_tool_evidence(project, run_id, &evidence_ids)
            .await?
    };
    let resolved = resolved
        .into_iter()
        .map(|evidence| (evidence.id(), evidence))
        .collect::<BTreeMap<_, _>>();
    let files = published.publication().graph().files();
    let stale_evidence_ids = evidence_ids
        .into_iter()
        .filter(|evidence_id| {
            resolved.get(evidence_id).is_none_or(|evidence| {
                let revision = evidence.location().revision();
                !files.iter().any(|current| current == revision)
            })
        })
        .collect();
    Ok(RecoveryMaterial {
        run,
        stored_ledger,
        published_snapshot_id: published.run().snapshot_id(),
        stale_evidence_ids,
        mutation_attempts,
        interrupted_tool_attempts,
    })
}

fn completed_verification_evidence(
    ledger: &TaskLedger,
) -> Result<Vec<TaskEvidenceId>, AgentRecoveryError> {
    let mut evidence = BTreeSet::new();
    for step in ledger
        .steps()
        .filter(|step| step.is_active_plan_step() && step.status() == TaskStepStatus::Completed)
    {
        if let Some(verification) = step
            .attempts()
            .iter()
            .rev()
            .filter_map(|attempt| attempt.verification())
            .find(|verification| verification.passed())
        {
            evidence.extend(verification.evidence_ids().iter().copied());
        }
        if evidence.len() > MAX_RECOVERY_EVIDENCE {
            return Err(AgentRecoveryError::ResourceLimit);
        }
    }
    Ok(evidence.into_iter().collect())
}

fn reopen_invalidated_steps(
    ledger: &mut TaskLedger,
    stale_evidence_ids: &[TaskEvidenceId],
    timestamp: TaskLedgerTimestamp,
) -> Result<Vec<TaskStepId>, TaskLedgerError> {
    let mut reopened = BTreeSet::new();
    for batch in stale_evidence_ids.chunks(INVALIDATION_BATCH_SIZE) {
        let invalidation = ledger.invalidate_verification_evidence(batch.to_vec(), timestamp)?;
        reopened.extend(invalidation.direct_step_ids().iter().copied());
        reopened.extend(invalidation.dependent_step_ids().iter().copied());
    }
    for step_id in &reopened {
        ledger.reopen_stale_step(*step_id, timestamp)?;
    }
    Ok(reopened.into_iter().collect())
}

fn cancel_active_step_for_recovery(
    ledger: &mut TaskLedger,
    choice: AgentRecoveryChoice,
    timestamp: TaskLedgerTimestamp,
) -> Result<(), AgentRecoveryError> {
    let active_step = ledger.steps().find_map(|step| {
        matches!(
            step.status(),
            TaskStepStatus::InProgress
                | TaskStepStatus::AwaitingApproval
                | TaskStepStatus::Verifying
        )
        .then_some(step.definition().id())
    });
    let Some(step_id) = active_step else {
        return Ok(());
    };
    let reason = match choice {
        AgentRecoveryChoice::Replan => "recovery requires a fresh plan",
        AgentRecoveryChoice::Cancel => "user cancelled the recovered run",
        AgentRecoveryChoice::Resume => return Ok(()),
    };
    ledger.cancel_step(
        step_id,
        TaskStepCancellationReason::try_from_string(reason.to_owned())
            .map_err(|_| AgentRecoveryError::InvalidRecoveryReason)?,
        timestamp,
    )?;
    Ok(())
}

/// Recovery failed before a safely current choice could be committed.
#[derive(Debug)]
pub enum AgentRecoveryError {
    /// The requested run was absent from this worktree.
    RunNotFound,
    /// The run already reached a terminal state.
    TerminalRun,
    /// The run's materialized Task Ledger was absent.
    LedgerNotFound,
    /// Goal or plan anchors did not agree across durable aggregates.
    AnchorMismatch,
    /// No atomically published index was available for freshness checks.
    PublishedIndexUnavailable,
    /// Resume was selected despite stale completed-verification evidence.
    ResumeRequiresReplan,
    /// A non-cancel recovery choice was requested before Unknown reconciliation.
    MutationReconciliationRequired,
    /// A timestamp could not cross both Run and Ledger persistence boundaries.
    InvalidTimestamp,
    /// An internal bounded recovery reason could not be represented.
    InvalidRecoveryReason,
    /// A fixed recovery evidence boundary was exceeded.
    ResourceLimit,
    /// Run-journal state could not be loaded.
    Journal(RunJournalStoreFailure),
    /// Task Ledger state could not be loaded.
    Ledger(TaskLedgerStoreFailure),
    /// Current published index state could not be loaded.
    Index(KnowledgeIndexFailure),
    /// Recovery-specific persistence failed.
    Store(AgentRecoveryStoreFailure),
    /// Domain Run invariants rejected the recovery event.
    RunDomain(AgentRunError),
    /// Domain Ledger invariants rejected invalidation or reopening.
    LedgerDomain(TaskLedgerError),
}

impl fmt::Display for AgentRecoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RunNotFound => "agent recovery run was not found",
            Self::TerminalRun => "terminal agent runs cannot be recovered",
            Self::LedgerNotFound => "agent recovery Task Ledger was not found",
            Self::AnchorMismatch => "agent recovery durable anchors do not match",
            Self::PublishedIndexUnavailable => {
                "agent recovery requires an atomically published index"
            }
            Self::ResumeRequiresReplan => {
                "agent recovery Resume requires fresh evidence and no pending mutation Replan"
            }
            Self::MutationReconciliationRequired => {
                "agent recovery requires Unknown mutation reconciliation before continuing"
            }
            Self::InvalidTimestamp => "agent recovery timestamp is invalid",
            Self::InvalidRecoveryReason => "agent recovery reason is invalid",
            Self::ResourceLimit => "agent recovery exceeded a fixed resource limit",
            Self::Journal(_) => "agent recovery could not load the run journal",
            Self::Ledger(_) => "agent recovery could not load the Task Ledger",
            Self::Index(_) => "agent recovery could not load the published index",
            Self::Store(_) => "agent recovery persistence failed",
            Self::RunDomain(_) => "agent recovery violated a Run invariant",
            Self::LedgerDomain(_) => "agent recovery violated a Task Ledger invariant",
        })
    }
}

impl Error for AgentRecoveryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Journal(error) => Some(error),
            Self::Ledger(error) => Some(error),
            Self::Index(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::RunDomain(error) => Some(error),
            Self::LedgerDomain(error) => Some(error),
            Self::RunNotFound
            | Self::TerminalRun
            | Self::LedgerNotFound
            | Self::AnchorMismatch
            | Self::PublishedIndexUnavailable
            | Self::ResumeRequiresReplan
            | Self::MutationReconciliationRequired
            | Self::InvalidTimestamp
            | Self::InvalidRecoveryReason
            | Self::ResourceLimit => None,
        }
    }
}

impl From<RunJournalStoreFailure> for AgentRecoveryError {
    fn from(value: RunJournalStoreFailure) -> Self {
        Self::Journal(value)
    }
}

impl From<TaskLedgerStoreFailure> for AgentRecoveryError {
    fn from(value: TaskLedgerStoreFailure) -> Self {
        Self::Ledger(value)
    }
}

impl From<KnowledgeIndexFailure> for AgentRecoveryError {
    fn from(value: KnowledgeIndexFailure) -> Self {
        Self::Index(value)
    }
}

impl From<AgentRecoveryStoreFailure> for AgentRecoveryError {
    fn from(value: AgentRecoveryStoreFailure) -> Self {
        Self::Store(value)
    }
}

impl From<AgentRunError> for AgentRecoveryError {
    fn from(value: AgentRunError) -> Self {
        Self::RunDomain(value)
    }
}

impl From<TaskLedgerError> for AgentRecoveryError {
    fn from(value: TaskLedgerError) -> Self {
        Self::LedgerDomain(value)
    }
}
