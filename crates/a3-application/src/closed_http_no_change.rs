//! Descriptive closed HTTP NoChange lineage; no dispatch, active anchor or mutation.
use crate::{
    AgentSourcePage, EditPhaseCallKind, EditPhaseObservation, EditPhaseObservationOutcome,
    EditPhaseReservationFailure as Failure, EditPhaseWireCost, HttpCorrectionOrigin,
    TaskLedgerStoreVersion,
};
use a3_domain::{
    AgentControllerState, AgentRunId, AgentRunTimestamp, ContentHash, EvidenceRef, FileRevision,
    GoalContractReference, GoalOriginalDigest, IndexRunId, ModelContextLimit, ModelOutputLimit,
    ModelProfile, ModelProfileReference, RunEvent, RunEventCode, RunEventId, RunEventKind,
    RunEventOutcome, RunEventSequence, SnapshotId, SourceRange, TaskLedgerRevision,
    TaskStepAttemptNumber, TaskStepId, VerificationSpecId, WorktreeId,
};

/// Historical edit coordinates only. These fields cannot construct an active
/// EditPhaseAnchor, a PreparedEditPhase, a dispatch permit or SQL authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedEditNoChangeOwner {
    /// Exact owning worktree.
    pub worktree: WorktreeId,
    /// Immutable original Goal revision.
    pub goal: GoalContractReference,
    /// Original bytes bound separately from the historical v3 job encoding.
    pub original_goal: GoalOriginalDigest,
    /// Same original Run, never a replacement owner.
    pub run: AgentRunId,
    /// Ledger revision of the real old replacement.
    pub ledger: TaskLedgerRevision,
    /// Store version before the atomic A closure.
    pub version: TaskLedgerStoreVersion,
    /// Actual old Original/ToolAction tail P+5, before closure P+6.
    pub sequence: RunEventSequence,
    /// First correction step which A actually closed.
    pub step: TaskStepId,
    /// Its exact old attempt number.
    pub attempt: TaskStepAttemptNumber,
    /// Original Addresses-diff specification.
    pub verification: VerificationSpecId,
    /// Exact publication, not merely its snapshot.
    pub index: IndexRunId,
    /// Unchanged source snapshot.
    pub snapshot: SnapshotId,
    /// Immutable model identity, separately checked with its complete profile.
    pub profile: ModelProfileReference,
}

/// Content-free exact Original interval and its actual supplied byte digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedEditWindow {
    original: EvidenceRef,
    digest: ContentHash,
}
impl ClosedEditWindow {
    /// Shape only; the adapter must prove the actual Original and full source.
    pub fn from_recorded(original: EvidenceRef, digest: ContentHash) -> Result<Self, Failure> {
        if original.range().is_empty()
            || original.range().len() > crate::edit_phase::MAX_EDIT_BYTES as u32
        {
            return Err(Failure::InvalidStoredData);
        }
        Ok(Self { original, digest })
    }
    /// Actual journaled Original, not accepted delivery credit.
    pub const fn original(&self) -> &EvidenceRef {
        &self.original
    }
    /// Complete unchanged file revision.
    pub const fn revision(&self) -> &FileRevision {
        self.original.revision()
    }
    /// Exact six byte/line coordinates.
    pub const fn range(&self) -> SourceRange {
        self.original.range()
    }
    /// Hash of the exact originally supplied replacement text.
    pub const fn digest(&self) -> ContentHash {
        self.digest
    }
}

/// One historical fully observed call, without any permission to repeat it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosedEditNoChangeCall {
    kind: EditPhaseCallKind,
    wire: EditPhaseWireCost,
    reserved_at: AgentRunTimestamp,
    deadline: AgentRunTimestamp,
    observation: EditPhaseObservation,
}
impl ClosedEditNoChangeCall {
    /// Shape only; SQL must prove terminal tag/receipt and genuine initial/repair history.
    pub fn from_recorded(
        kind: EditPhaseCallKind,
        wire: EditPhaseWireCost,
        reserved_at: AgentRunTimestamp,
        deadline: AgentRunTimestamp,
        observation: EditPhaseObservation,
    ) -> Result<Self, Failure> {
        if observation.outcome() != EditPhaseObservationOutcome::NoChange
            || observation.finished_at() < reserved_at
            || observation.finished_at() >= deadline
            || observation
                .reported_prompt_tokens()
                .is_none_or(|n| n == 0 || n > wire.prompt_estimate())
            || observation.reported_output_tokens().is_none_or(|n| n == 0)
        {
            return Err(Failure::InvalidStoredData);
        }
        Ok(Self {
            kind,
            wire,
            reserved_at,
            deadline,
            observation,
        })
    }
    /// Actual call kind.
    pub const fn kind(&self) -> EditPhaseCallKind {
        self.kind
    }
    /// Observed full request and original reservations.
    pub const fn wire(&self) -> EditPhaseWireCost {
        self.wire
    }
    /// Original reserve timestamp.
    pub const fn reserved_at(&self) -> AgentRunTimestamp {
        self.reserved_at
    }
    /// Original absolute Run deadline.
    pub const fn deadline(&self) -> AgentRunTimestamp {
        self.deadline
    }
    /// Positive observed terminal usage and time.
    pub const fn observation(&self) -> EditPhaseObservation {
        self.observation
    }
}

/// Recorded capacity values; reference equality alone does not replace full-profile checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosedNoChangeProfileLimits {
    context: ModelContextLimit,
    generation: ModelOutputLimit,
}
impl ClosedNoChangeProfileLimits {
    /// Captures typed settings, without asserting profile origin or probe success.
    pub fn from_profile(profile: &ModelProfile) -> Result<Self, Failure> {
        Self::from_recorded(
            profile.settings().context_limit(),
            profile.settings().output_limit(),
        )
    }
    /// Requires space for a positive prompt under the original actual context.
    pub fn from_recorded(
        context: ModelContextLimit,
        generation: ModelOutputLimit,
    ) -> Result<Self, Failure> {
        if generation.get() >= context.get() {
            return Err(Failure::InvalidStoredData);
        }
        Ok(Self {
            context,
            generation,
        })
    }
    /// Actual context from the immutable profile.
    pub const fn context_limit(&self) -> ModelContextLimit {
        self.context
    }
    /// Actual generation ceiling, not the22-percent reserve.
    pub const fn generation_limit(&self) -> ModelOutputLimit {
        self.generation
    }
}

/// Descriptive closed byte-identity fact. Only its dedicated store read purpose
/// can prove actual rows, closure, abandoned1, failed attempt and currentness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedEditNoChangeFact {
    owner: ClosedEditNoChangeOwner,
    window: ClosedEditWindow,
    job: ContentHash,
    source_read: ContentHash,
    call: ClosedEditNoChangeCall,
    limits: ClosedNoChangeProfileLimits,
}
impl ClosedEditNoChangeFact {
    /// Checks bounded forms and exact capacity math, never persistence truth.
    pub fn from_recorded(
        owner: ClosedEditNoChangeOwner,
        window: ClosedEditWindow,
        job: ContentHash,
        source_read: ContentHash,
        call: ClosedEditNoChangeCall,
        limits: ClosedNoChangeProfileLimits,
    ) -> Result<Self, Failure> {
        let reserve = u32::try_from((u64::from(limits.context_limit().get()) * 22).div_ceil(100))
            .map_err(|_| Failure::InvalidStoredData)?
            .max(limits.generation_limit().get());
        if call
            .observation()
            .reported_output_tokens()
            .is_none_or(|n| n > limits.generation_limit().get())
            || call.wire().output_reserve() != reserve
            || call
                .wire()
                .prompt_estimate()
                .checked_add(reserve)
                .is_none_or(|n| n > limits.context_limit().get())
        {
            return Err(Failure::InvalidStoredData);
        }
        Ok(Self {
            owner,
            window,
            job,
            source_read,
            call,
            limits,
        })
    }
    /// Exact historical owner; not a live edit anchor.
    pub const fn owner(&self) -> &ClosedEditNoChangeOwner {
        &self.owner
    }
    /// Actual old replacement original and digest.
    pub const fn window(&self) -> &ClosedEditWindow {
        &self.window
    }
    /// Historical v3 job to be independently replayed against current bytes.
    pub const fn job_digest(&self) -> ContentHash {
        self.job
    }
    /// Actual old acquisition, now closed without accepted delivery credit.
    pub const fn source_read_key(&self) -> ContentHash {
        self.source_read
    }
    /// Actual historical call and usage.
    pub const fn call(&self) -> &ClosedEditNoChangeCall {
        &self.call
    }
    /// Original capacity values, independently matched to the complete profile by storage.
    pub const fn limits(&self) -> ClosedNoChangeProfileLimits {
        self.limits
    }
    /// Exact A-event identity. This deterministic shape is not evidence it was committed.
    pub fn closure_event_id(&self) -> RunEventId {
        crate::edit_no_change::no_change_closure_event_id(
            self.owner.run,
            self.job,
            self.call.wire(),
            self.source_read,
            self.owner.index,
            self.call.kind(),
        )
    }
}

/// Genuine first HTTP producer and one real A closure, as descriptive typed data.
/// It grants neither a later plan dispatch nor an edit/source-delivery capability.
#[derive(Debug, Clone)]
pub struct ClosedHttpNoChangeOrigin {
    parent: HttpCorrectionOrigin,
    fact: ClosedEditNoChangeFact,
    closure: RunEvent,
    closed_version: TaskLedgerStoreVersion,
}
impl ClosedHttpNoChangeOrigin {
    /// Shape only. Store must independently authenticate the actual first producer,
    /// full known graph, source/read/row history, closure and exact current frame.
    pub fn from_recorded(
        parent: HttpCorrectionOrigin,
        fact: ClosedEditNoChangeFact,
        closure: RunEvent,
        closed_version: TaskLedgerStoreVersion,
    ) -> Result<Self, Failure> {
        let a = &parent.anchor;
        let owner = fact.owner();
        let p = parent
            .producer
            .application_tail(a)
            .map_err(|_| Failure::InvalidStoredData)?;
        if owner.worktree != a.worktree
            || owner.goal != a.goal
            || owner.original_goal != a.original
            || owner.run != a.run
            || owner.index != a.index
            || owner.snapshot != a.snapshot
            || owner.profile != a.profile
            || fact.window().revision() != &a.source
            || a.ledger.get().checked_add(1) != Some(owner.ledger.get())
            || p.get().checked_add(5) != Some(owner.sequence.get())
            || owner.version.get().checked_add(1) != Some(closed_version.get())
            || owner.sequence.get().checked_add(1) != Some(closure.sequence().get())
            || closure.id() != fact.closure_event_id()
            || closure.run_id() != owner.run
            || closure.snapshot_id() != owner.snapshot
            || closure.occurred_at() < fact.call().observation().finished_at()
            || closure.occurred_at() >= fact.call().deadline()
            || !matches!(
                closure.kind(),
                RunEventKind::StateTransition {
                    from: AgentControllerState::Execute,
                    to: AgentControllerState::Replan
                }
            )
            || closure.payload().code() != RunEventCode::ControllerDecision
            || closure.payload().outcome() != Some(RunEventOutcome::Succeeded)
            || closure.payload().redaction().is_some()
            || closure.subject().is_some()
            || closure.turn_charge().is_some()
        {
            return Err(Failure::InvalidStoredData);
        }
        Ok(Self {
            parent,
            fact,
            closure,
            closed_version,
        })
    }
    /// Original Direct or real S1/Context producer, never a manufactured V4 focus.
    pub const fn parent(&self) -> &HttpCorrectionOrigin {
        &self.parent
    }
    /// Exact closed NoChange fact, never the current live receipt.
    pub const fn fact(&self) -> &ClosedEditNoChangeFact {
        &self.fact
    }
    /// Exact A-closure event after the observed failed edit.
    pub const fn closure(&self) -> &RunEvent {
        &self.closure
    }
    /// Current A-closed Ledger store version, without a new Ledger revision.
    pub const fn closed_version(&self) -> TaskLedgerStoreVersion {
        self.closed_version
    }
}

/// Replays the unchanged legacy v3 byte encoding from one safely reread exact
/// Original. Caller/adapter still prove freshness and actual persisted history.
/// No live anchor, Run, Execute state, phase, permit or model request is constructed.
pub fn replay_closed_no_change_edit_job(
    fact: &ClosedEditNoChangeFact,
    page: &AgentSourcePage,
) -> Result<ContentHash, Failure> {
    let window = fact.window();
    let evidence = EvidenceRef::new(page.revision().clone(), page.range());
    if &evidence != window.original()
        || usize::try_from(page.range().len()).ok() != Some(page.text().len())
        || page.range().start_position().column() != 0
        || page.start_line().get().checked_sub(1) != Some(page.range().start_position().row())
        || blake3::hash(page.text().as_bytes()).as_bytes() != window.digest().as_bytes()
    {
        return Err(Failure::InvalidStoredData);
    }
    let a = fact.owner();
    let owner = crate::edit_job_digest::EditJobOwnerProjection {
        worktree: a.worktree,
        goal: a.goal,
        ledger: a.ledger,
        version: a.version,
        run: a.run,
        sequence: a.sequence,
        step: a.step,
        attempt: a.attempt,
        verification: a.verification,
        snapshot: a.snapshot,
        empty_published_index: false,
        planned_file_addition_scope: None,
        model_profile: a.profile,
    };
    let actual = ContentHash::from_bytes(
        *crate::edit_job_digest::existing_target_digest(
            &owner,
            window.revision(),
            u64::from(window.range().start_byte()),
            u64::from(window.range().end_byte()),
            page.text(),
            false,
        )
        .as_bytes(),
    );
    if actual != fact.job_digest() {
        return Err(Failure::InvalidStoredData);
    }
    Ok(actual)
}
