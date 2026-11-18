//! One exact source window and one small reply compiled into the existing patch action.

use crate::edit_scope_frame::{EditOwnerClock, EditScopeFrame};
use crate::{
    AdmitSourceTaskStep, AgentSourcePage, CompiledEditPhaseRequest, LanguageAdapter,
    PatchPreviewFailure, StoredTaskLedger, TaskLedgerStoreVersion, WorkspacePatchControl,
    WorkspacePatchTool,
};
use a3_domain::{
    AgentAction, AgentControllerState, AgentRun, AgentRunId, AgentRunTimestamp, ContentHash,
    EvidenceRef, FileRevision, GoalContract, GoalContractReference, ModelProfileReference,
    PatchAction, PatchActionSchemaVersion, PatchAdd, PatchFileContent, PatchLineEndings,
    PatchOperation, PatchOperationError, PatchRationale, PatchTextEncoding, PatchUpdate,
    PlannedFileAddition, ProjectIdentity, PublishedIndex, RepositoryPath, RunEventSequence,
    SnapshotId, SourceRange, TaskLedgerRevision, TaskStepAttemptNumber, TaskStepAttemptOutcome,
    TaskStepId, TaskStepStatus, VerificationSpecId, WorktreeId,
};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, ops::Range, sync::Arc};

pub(crate) const MAX_EDIT_BYTES: usize = 8 * 1024;
pub(crate) const MAX_EDIT_PHASE_REPLY_BYTES: usize = 6 * MAX_EDIT_BYTES + 128;

/// Content-free owner of a single edit-phase job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditPhaseAnchor {
    worktree: WorktreeId,
    goal: GoalContractReference,
    ledger: TaskLedgerRevision,
    version: TaskLedgerStoreVersion,
    run: AgentRunId,
    sequence: RunEventSequence,
    step: TaskStepId,
    attempt: TaskStepAttemptNumber,
    verification: VerificationSpecId,
    snapshot: SnapshotId,
    empty_published_index: bool,
    planned_file_addition_scope: Option<ContentHash>,
    model_profile: ModelProfileReference,
}

impl EditPhaseAnchor {
    /// Returns the exact worktree owner.
    #[must_use]
    pub const fn worktree(self) -> WorktreeId {
        self.worktree
    }

    /// Returns the original Goal revision.
    #[must_use]
    pub const fn goal(self) -> GoalContractReference {
        self.goal
    }

    /// Returns the active plan revision.
    #[must_use]
    pub const fn ledger(self) -> TaskLedgerRevision {
        self.ledger
    }

    /// Returns the loaded Ledger CAS version.
    #[must_use]
    pub const fn version(self) -> TaskLedgerStoreVersion {
        self.version
    }

    /// Returns the owning run.
    #[must_use]
    pub const fn run(self) -> AgentRunId {
        self.run
    }

    /// Returns the exact current journal position.
    #[must_use]
    pub const fn sequence(self) -> RunEventSequence {
        self.sequence
    }

    /// Returns the active task step.
    #[must_use]
    pub const fn step(self) -> TaskStepId {
        self.step
    }

    /// Returns the active immutable attempt number.
    #[must_use]
    pub const fn attempt(self) -> TaskStepAttemptNumber {
        self.attempt
    }

    /// Returns the current verification obligation.
    #[must_use]
    pub const fn verification(self) -> VerificationSpecId {
        self.verification
    }

    /// Returns the exact published source snapshot.
    #[must_use]
    pub const fn snapshot(self) -> SnapshotId {
        self.snapshot
    }

    /// Whether the bound published graph contains no file revisions.
    #[must_use]
    pub const fn empty_published_index(self) -> bool {
        self.empty_published_index
    }

    /// Exact optional operation scope of the current immutable step definition.
    #[must_use]
    pub const fn planned_file_addition_scope(self) -> Option<ContentHash> {
        self.planned_file_addition_scope
    }

    /// Returns the model profile pinned by the run.
    #[must_use]
    pub const fn model_profile(self) -> ModelProfileReference {
        self.model_profile
    }

    fn target_digest(self, target: &EditTarget) -> blake3::Hash {
        let owner = crate::edit_job_digest::EditJobOwnerProjection {
            worktree: self.worktree,
            goal: self.goal,
            ledger: self.ledger,
            version: self.version,
            run: self.run,
            sequence: self.sequence,
            step: self.step,
            attempt: self.attempt,
            verification: self.verification,
            snapshot: self.snapshot,
            empty_published_index: self.empty_published_index,
            planned_file_addition_scope: self.planned_file_addition_scope,
            model_profile: self.model_profile,
        };
        match target {
            EditTarget::Existing(window) => crate::edit_job_digest::existing_target_digest(
                &owner,
                &window.revision,
                window.range.start as u64,
                window.range.end as u64,
                &window.exact,
                window.append,
            ),
            EditTarget::Add(path) => crate::edit_job_digest::add_target_digest(&owner, path),
        }
    }
}

/// Current Core-owned execution state for the candidate's active edit phase.
#[derive(Clone, Copy)]
pub struct EditPhaseState<'a> {
    project: &'a ProjectIdentity,
    published: &'a PublishedIndex,
    anchor: EditPhaseAnchor,
    planned_file_addition: Option<&'a PlannedFileAddition>,
    deadline: AgentRunTimestamp,
}

impl<'a> EditPhaseState<'a> {
    /// Binds one active Source attempt to the current published snapshot and store revision.
    pub fn new(
        project: &'a ProjectIdentity,
        goal: &'a GoalContract,
        stored: &'a StoredTaskLedger,
        run: &'a AgentRun,
        published: &'a PublishedIndex,
    ) -> Result<Self, EditPhaseFailure> {
        let ledger = stored.ledger();
        let snapshot = published.run().snapshot_id();
        if goal.original().is_none()
            || run.state() != AgentControllerState::Execute
            || run.goal_contract() != goal.reference()
            || ledger.goal_contract() != goal.reference()
            || run.task_ledger_revision() != ledger.revision()
            || run.current_snapshot_id() != snapshot
            || ledger.pending_planning().is_some()
        {
            return Err(EditPhaseFailure::InactiveOrStale);
        }
        let mut active = ledger.steps().filter(|step| {
            step.is_active_plan_step() && step.status() == TaskStepStatus::InProgress
        });
        let step = active.next().ok_or(EditPhaseFailure::InactiveOrStale)?;
        if active.next().is_some() {
            return Err(EditPhaseFailure::InactiveOrStale);
        }
        let attempt = step
            .attempts()
            .last()
            .filter(|attempt| {
                attempt.run_id() == run.id()
                    && matches!(attempt.outcome(), TaskStepAttemptOutcome::Active)
            })
            .ok_or(EditPhaseFailure::InactiveOrStale)?;
        AdmitSourceTaskStep
            .execute(goal, step.definition())
            .map_err(|_| EditPhaseFailure::InactiveOrStale)?;
        Ok(Self {
            project,
            published,
            deadline: AgentRunTimestamp::from_unix_millis(
                run.created_at()
                    .unix_millis()
                    .checked_add(run.budget().duration_limit().millis())
                    .ok_or(EditPhaseFailure::DeadlineReached)?,
            )
            .map_err(|_| EditPhaseFailure::DeadlineReached)?,
            anchor: EditPhaseAnchor {
                worktree: project.worktree().id(),
                goal: goal.reference(),
                ledger: ledger.revision(),
                version: stored.version(),
                run: run.id(),
                sequence: run.last_event_sequence(),
                step: step.definition().id(),
                attempt: attempt.number(),
                verification: step.definition().verification_spec().id(),
                snapshot,
                empty_published_index: published.publication().graph().files().is_empty(),
                planned_file_addition_scope: step
                    .definition()
                    .planned_file_addition()
                    .map(PlannedFileAddition::scope_digest),
                model_profile: run
                    .model_profile()
                    .ok_or(EditPhaseFailure::InactiveOrStale)?,
            },
            planned_file_addition: step.definition().planned_file_addition(),
        })
    }

    /// Returns the validated content-free attempt anchor.
    #[must_use]
    pub const fn anchor(self) -> EditPhaseAnchor {
        self.anchor
    }

    pub(crate) const fn project(self) -> &'a ProjectIdentity {
        self.project
    }

    pub(crate) const fn published(self) -> &'a PublishedIndex {
        self.published
    }

    pub(crate) const fn deadline(self) -> AgentRunTimestamp {
        self.deadline
    }

    fn revision_at(self, path: &RepositoryPath) -> Option<&'a FileRevision> {
        let files = self.published.publication().graph().files();
        let position = files.binary_search_by(|file| file.path().cmp(path)).ok()?;
        files.get(position)
    }
}

impl fmt::Debug for EditPhaseState<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EditPhaseState")
            .field("anchor", &self.anchor)
            .finish_non_exhaustive()
    }
}

struct ExistingWindow {
    revision: FileRevision,
    range: Range<usize>,
    exact: String,
    append: bool,
}

impl ExistingWindow {
    fn targets_declaration(&self, revision: &FileRevision, declaration: SourceRange) -> bool {
        if self.append || &self.revision != revision {
            return false;
        }
        let start = self.range.start.max(declaration.start_byte() as usize);
        let end = self.range.end.min(declaration.end_byte() as usize);
        if start >= end {
            return false;
        }
        let prefix_end = start - self.range.start;
        let suffix_start = end - self.range.start;
        let bytes = self.exact.as_bytes();
        bytes
            .get(..prefix_end)
            .zip(bytes.get(suffix_start..))
            .is_some_and(|(prefix, suffix)| {
                prefix
                    .iter()
                    .chain(suffix)
                    .all(|byte| matches!(*byte, b' ' | b'\t' | b'\r' | b'\n'))
            })
    }
}

/// Core-selected operation on the exact bound edit window; never a model-selected target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditWindowKind {
    /// Replaces the entire supplied current window.
    Replace,
    /// Inserts only a new suffix after the supplied EOF context.
    Append,
    /// Supplies the complete content for the selected new file.
    Add,
}

enum EditTarget {
    Existing(ExistingWindow),
    Add(RepositoryPath),
}

/// Single-use reply binding and exact serialized input to a candidate edit phase.
pub struct PreparedEditPhase {
    anchor: EditPhaseAnchor,
    target: EditTarget,
    job_digest: ContentHash,
    input: String,
    original_page: Option<EvidenceRef>,
    source_publication: Option<a3_domain::IndexRunId>,
    scope: EditScopeBinding,
    clock: Option<EditOwnerClock>,
    language: Option<Arc<dyn LanguageAdapter>>,
}

#[derive(Debug)]
enum EditScopeBinding {
    Unqualified,
    Required,
    Qualified(EditScopeFrame),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditReplyProtocol {
    V3,
}

/// A bounded version-3 reply attached by the Core to its dispatch owner.
///
/// Model JSON cannot construct or change this routing binding. The envelope is descriptive:
/// only current-state revalidation and the existing patch boundary can authorize a mutation.
pub struct BoundEditPhaseReply<'a> {
    protocol: EditReplyProtocol,
    job_digest: ContentHash,
    bytes: &'a [u8],
    request: Option<ContentHash>,
    frame: Option<ContentHash>,
}

impl<'a> BoundEditPhaseReply<'a> {
    fn new(job_digest: ContentHash, bytes: &'a [u8]) -> Result<Self, EditPhaseFailure> {
        if bytes.len() > MAX_EDIT_PHASE_REPLY_BYTES {
            return Err(EditPhaseFailure::TooLarge);
        }
        Ok(Self {
            protocol: EditReplyProtocol::V3,
            job_digest,
            bytes,
            request: None,
            frame: None,
        })
    }

    fn decode(self, expected_job: ContentHash) -> Result<DecodedEditReply, EditPhaseFailure> {
        match self.protocol {
            EditReplyProtocol::V3 if self.job_digest == expected_job => decode_reply(self.bytes),
            EditReplyProtocol::V3 => Err(EditPhaseFailure::WrongReplyBinding),
        }
    }
}

impl fmt::Debug for BoundEditPhaseReply<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundEditPhaseReply")
            .field("protocol", &self.protocol)
            .field("job_digest", &self.job_digest)
            .field("reply_bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

impl PreparedEditPhase {
    /// Offers the exact current source page as the one editable window.
    pub fn update(
        state: EditPhaseState<'_>,
        page: &AgentSourcePage,
    ) -> Result<Self, EditPhaseFailure> {
        Self::existing(state, page, false)
    }

    /// Offers an insertion at the end of a page that actually reaches the file's end.
    pub fn append(
        state: EditPhaseState<'_>,
        page: &AgentSourcePage,
    ) -> Result<Self, EditPhaseFailure> {
        Self::existing(state, page, true)
    }

    /// Offers a small new file at a Core-selected path. E3 still checks live absence and scope.
    pub fn add(state: EditPhaseState<'_>, path: RepositoryPath) -> Result<Self, EditPhaseFailure> {
        if state.revision_at(&path).is_some()
            || state
                .planned_file_addition
                .is_some_and(|addition| addition.target() != &path)
        {
            return Err(EditPhaseFailure::InvalidWindow);
        }
        Self::new(state.anchor, EditTarget::Add(path))
    }

    fn existing(
        state: EditPhaseState<'_>,
        page: &AgentSourcePage,
        append: bool,
    ) -> Result<Self, EditPhaseFailure> {
        if state.planned_file_addition.is_some() {
            return Err(EditPhaseFailure::InvalidWindow);
        }
        if state.revision_at(page.revision().path()) != Some(page.revision()) {
            return Err(EditPhaseFailure::InactiveOrStale);
        }
        let mut start = page.range().start_byte() as usize;
        let end = page.range().end_byte() as usize;
        let mut exact = page.text();
        if end - start != exact.len() || (append && page.truncated()) {
            return Err(EditPhaseFailure::InvalidWindow);
        }
        if start == 0
            && let Some(without_bom) = exact.strip_prefix('\u{feff}')
        {
            start += '\u{feff}'.len_utf8();
            exact = without_bom;
        }
        if !append && exact.is_empty() {
            return Err(EditPhaseFailure::InvalidWindow);
        }
        if exact.len() > MAX_EDIT_BYTES {
            return Err(EditPhaseFailure::TooLarge);
        }
        let mut phase = Self::new(
            state.anchor,
            EditTarget::Existing(ExistingWindow {
                revision: page.revision().clone(),
                range: start..end,
                exact: exact.to_owned(),
                append,
            }),
        )?;
        if !append {
            phase.source_publication = Some(state.published().run().id());
            // The original complete Tool page is independent of wire delivery. BOM
            // bytes remain outside the normalized editable window and are preserved.
            phase.original_page = Some(EvidenceRef::new(page.revision().clone(), page.range()));
            if crate::edit_scope_frame::eligible(
                state,
                phase
                    .original_page
                    .as_ref()
                    .ok_or(EditPhaseFailure::InvalidWindow)?,
            ) {
                phase.scope = EditScopeBinding::Required;
            }
        }
        Ok(phase)
    }

    fn new(anchor: EditPhaseAnchor, target: EditTarget) -> Result<Self, EditPhaseFailure> {
        let digest = anchor.target_digest(&target);
        let (operation, current) = match &target {
            EditTarget::Existing(window) if window.append => ("append", window.exact.as_str()),
            EditTarget::Existing(window) => ("replace", window.exact.as_str()),
            EditTarget::Add(_) => ("add", ""),
        };
        let input = serde_json::to_string(&EditInput {
            version: 3,
            operation,
            current,
            inventory: (anchor.empty_published_index && matches!(&target, EditTarget::Add(_)))
                .then_some("emptyPublishedIndex"),
        })
        .map_err(|_| EditPhaseFailure::InvalidWindow)?;
        Ok(Self {
            anchor,
            target,
            job_digest: ContentHash::from_bytes(*digest.as_bytes()),
            input,
            original_page: None,
            source_publication: None,
            scope: EditScopeBinding::Unqualified,
            clock: None,
            language: None,
        })
    }

    /// Returns the exact wire fragment to supply to the phase. JSON escaping preserves raw bytes.
    #[must_use]
    pub fn input_json(&self) -> &str {
        &self.input
    }

    /// Returns the exact phase owner without retaining raw source in durable metadata.
    #[must_use]
    pub const fn anchor(&self) -> EditPhaseAnchor {
        self.anchor
    }

    /// Returns the full Core-derived job identity. Repair retains the same identity.
    #[must_use]
    pub const fn job_digest(&self) -> ContentHash {
        self.job_digest
    }

    /// Describes only the exact non-append existing replacement. This is not a
    /// NoChange observation, source delivery, currentness check or mutation permit.
    pub fn no_change_window(&self) -> Option<crate::EditNoChangeWindow> {
        let EditTarget::Existing(window) = &self.target else {
            return None;
        };
        if window.append {
            return None;
        }
        crate::EditNoChangeWindow::from_window(
            window.revision.clone(),
            u32::try_from(window.range.start).ok()?,
            u32::try_from(window.range.end).ok()?,
            ContentHash::from_bytes(*blake3::hash(window.exact.as_bytes()).as_bytes()),
        )
    }

    /// Attaches a bounded provider reply to this Core-owned job without decoding it.
    /// The provider dispatcher calls this on the phase that owned the actual request.
    pub fn bind_reply<'a>(
        &self,
        bytes: &'a [u8],
    ) -> Result<BoundEditPhaseReply<'a>, EditPhaseFailure> {
        if !matches!(self.scope, EditScopeBinding::Unqualified) {
            return Err(EditPhaseFailure::WrongReplyBinding);
        }
        BoundEditPhaseReply::new(self.job_digest, bytes)
    }

    /// Borrows the current existing target revision, without any read or mutation authority.
    #[must_use]
    pub fn target_revision(&self) -> Option<&FileRevision> {
        match &self.target {
            EditTarget::Existing(window) => Some(&window.revision),
            EditTarget::Add(_) => None,
        }
    }

    /// Original publication of an Existing/Replace source; not a frame or source credit.
    #[must_use]
    pub const fn source_publication(&self) -> Option<a3_domain::IndexRunId> {
        self.source_publication
    }

    /// Captures any mandatory complete-callable frame before reservation or provider I/O.
    /// The consumed phase owns the original clock across qualification and Execute.
    pub async fn prepare_scope_frame(
        self,
        compiled: &mut CompiledEditPhaseRequest,
        current: EditPhaseState<'_>,
        adapter: Option<Arc<dyn LanguageAdapter>>,
        observed_at: AgentRunTimestamp,
        patch_tool: &dyn WorkspacePatchTool,
        control: &dyn WorkspacePatchControl,
    ) -> Result<Self, EditPhaseFailure> {
        let phase = self
            .qualify_scope_frame(compiled, current, adapter, observed_at, patch_tool, control)
            .await?;
        if compiled
            .scope_frame_digest()
            .is_some_and(|old| Some(old) != phase.scope_frame_digest())
        {
            return Err(EditPhaseFailure::WrongReplyBinding);
        }
        compiled.bind_scope_frame(phase.scope_frame_digest(), phase.scope_frame_index());
        phase.require_scope_binding(compiled)?;
        Ok(phase)
    }

    /// Reproves exactly the already-bound current frame for a repair or cold request.
    /// It cannot introduce a frame into an old unqualified request or change its identity.
    pub async fn prepare_bound_scope_frame(
        self,
        compiled: &CompiledEditPhaseRequest,
        current: EditPhaseState<'_>,
        adapter: Option<Arc<dyn LanguageAdapter>>,
        observed_at: AgentRunTimestamp,
        patch_tool: &dyn WorkspacePatchTool,
        control: &dyn WorkspacePatchControl,
    ) -> Result<Self, EditPhaseFailure> {
        let phase = self
            .qualify_scope_frame(compiled, current, adapter, observed_at, patch_tool, control)
            .await?;
        phase.require_scope_binding(compiled)?;
        Ok(phase)
    }

    async fn qualify_scope_frame(
        mut self,
        compiled: &CompiledEditPhaseRequest,
        current: EditPhaseState<'_>,
        adapter: Option<Arc<dyn LanguageAdapter>>,
        observed_at: AgentRunTimestamp,
        patch_tool: &dyn WorkspacePatchTool,
        control: &dyn WorkspacePatchControl,
    ) -> Result<Self, EditPhaseFailure> {
        if self
            .source_publication
            .is_some_and(|index| index != current.published().run().id())
        {
            return Err(EditPhaseFailure::InactiveOrStale);
        }
        if let Some(clock) = self.clock {
            self.require_scope_binding(compiled)?;
            clock.check(control)?;
            return Ok(self); // same immutable frame/port/clock; never a retry or time reset
        }
        let clock = EditOwnerClock::new(observed_at, current.deadline());
        if self.anchor != current.anchor()
            || compiled.anchor() != self.anchor
            || compiled.job_digest() != self.job_digest
        {
            return Err(EditPhaseFailure::WrongReplyBinding);
        }
        self.clock = Some(clock);
        clock.check(control)?;
        if matches!(self.scope, EditScopeBinding::Required) {
            let selected = adapter.ok_or(EditPhaseFailure::ScopeFrameUnproven)?;
            let adapter = selected.as_ref();
            let page = self
                .original_page
                .as_ref()
                .ok_or(EditPhaseFailure::ScopeFrameUnproven)?;
            let EditTarget::Existing(window) = &self.target else {
                return Err(EditPhaseFailure::ScopeFrameUnproven);
            };
            let scoped = clock.control(control);
            let base = read_window_base(window, current, patch_tool, &scoped).await;
            clock.check(control)?;
            let base = base?;
            existing_line_endings(window, &base)?;
            let leading = u32::try_from(window.range.start)
                .ok()
                .and_then(|start| start.checked_sub(page.range().start_byte()))
                .ok_or(EditPhaseFailure::InvalidWindow)?;
            if leading != 0
                && !(leading == 3
                    && page.range().start_byte() == 0
                    && base.encoding() == PatchTextEncoding::Utf8Bom)
            {
                return Err(EditPhaseFailure::InvalidWindow);
            }
            let start = page.range().start_position();
            let editable = SourceRange::new(
                window.range.start,
                window.range.end,
                a3_domain::SourcePosition::new(
                    start.row(),
                    start
                        .column()
                        .checked_add(leading)
                        .ok_or(EditPhaseFailure::InvalidWindow)?,
                ),
                page.range().end_position(),
            )
            .map_err(|_| EditPhaseFailure::InvalidWindow)?;
            self.scope = EditScopeBinding::Qualified(EditScopeFrame::capture(
                current,
                page,
                editable,
                base.as_bytes(),
                adapter,
                clock,
                control,
            )?);
            self.language = Some(selected);
        }
        Ok(self)
    }

    /// Requires the same mandatory Core frame as the exact compiled request.
    /// Direct reservation and legacy completion cannot skip a required qualification.
    pub fn require_scope_binding(
        &self,
        compiled: &CompiledEditPhaseRequest,
    ) -> Result<(), EditPhaseFailure> {
        if self.anchor != compiled.anchor()
            || self.job_digest != compiled.job_digest()
            || self.source_publication != compiled.source_publication()
        {
            return Err(EditPhaseFailure::WrongReplyBinding);
        }
        if matches!(self.scope, EditScopeBinding::Required) {
            return Err(EditPhaseFailure::ScopeFrameUnproven);
        }
        if self.scope_frame_digest() != compiled.scope_frame_digest()
            || self.scope_frame_index() != compiled.scope_frame_index()
        {
            return Err(EditPhaseFailure::WrongReplyBinding);
        }
        Ok(())
    }
    /// Rejects both pending assessment and qualified jobs at old no-request reservation entries.
    /// This cannot waive a Core-derived mandatory qualification.
    pub fn require_unqualified_scope(&self) -> Result<(), EditPhaseFailure> {
        if matches!(self.scope, EditScopeBinding::Unqualified) {
            Ok(())
        } else {
            Err(EditPhaseFailure::ScopeFrameUnproven)
        }
    }

    fn scope_frame_index(&self) -> Option<a3_domain::IndexRunId> {
        match &self.scope {
            EditScopeBinding::Qualified(frame) => Some(frame.index()),
            _ => None,
        }
    }
    pub(crate) fn scope_frame_digest(&self) -> Option<ContentHash> {
        match &self.scope {
            EditScopeBinding::Qualified(frame) => Some(frame.digest()),
            _ => None,
        }
    }
    pub(crate) fn owner_clock(
        &self,
        at: AgentRunTimestamp,
        current: EditPhaseState<'_>,
    ) -> Result<EditOwnerClock, EditPhaseFailure> {
        match self.clock {
            Some(clock) if clock.deadline() == current.deadline() => Ok(clock),
            Some(_) => Err(EditPhaseFailure::WrongReplyBinding),
            None if matches!(self.scope, EditScopeBinding::Unqualified) => {
                Ok(EditOwnerClock::new(at, current.deadline()))
            }
            None => Err(EditPhaseFailure::ScopeFrameUnproven),
        }
    }
    pub(crate) fn require_current_scope(
        &self,
        current: EditPhaseState<'_>,
    ) -> Result<(), EditPhaseFailure> {
        if self
            .source_publication
            .is_some_and(|index| index != current.published().run().id())
        {
            return Err(EditPhaseFailure::InactiveOrStale);
        }
        match &self.scope {
            EditScopeBinding::Required => Err(EditPhaseFailure::ScopeFrameUnproven),
            EditScopeBinding::Qualified(frame) => frame.require_current(current),
            EditScopeBinding::Unqualified => {
                if self
                    .original_page
                    .as_ref()
                    .is_some_and(|page| crate::edit_scope_frame::eligible(current, page))
                {
                    Err(EditPhaseFailure::ScopeFrameUnproven)
                } else {
                    Ok(())
                }
            }
        }
    }
    pub(crate) fn bind_request_reply<'a>(
        &self,
        compiled: &CompiledEditPhaseRequest,
        bytes: &'a [u8],
    ) -> Result<BoundEditPhaseReply<'a>, EditPhaseFailure> {
        self.require_scope_binding(compiled)?;
        let mut reply = BoundEditPhaseReply::new(self.job_digest, bytes)?;
        reply.request = Some(compiled.request_digest());
        reply.frame = self.scope_frame_digest();
        Ok(reply)
    }

    /// Operation-specific instruction identity selected by the existing bound target.
    pub(crate) fn kind(&self) -> EditWindowKind {
        match &self.target {
            EditTarget::Existing(window) if window.append => EditWindowKind::Append,
            EditTarget::Existing(_) => EditWindowKind::Replace,
            EditTarget::Add(_) => EditWindowKind::Add,
        }
    }

    pub(crate) fn targets_declaration(
        &self,
        revision: &FileRevision,
        declaration: SourceRange,
    ) -> bool {
        matches!(&self.target, EditTarget::Existing(window) if window.targets_declaration(revision, declaration))
    }

    /// Requires the identical current non-append target span; no overlapping-window rebinding.
    pub(crate) fn targets_exact_span(&self, required: &a3_domain::EvidenceRef) -> bool {
        matches!(&self.target, EditTarget::Existing(window)
            if !window.append && &window.revision == required.revision()
                && window.range.start == required.range().start_byte() as usize
                && window.range.end == required.range().end_byte() as usize)
    }

    pub(crate) fn updates_revision(&self, revision: &FileRevision) -> bool {
        matches!(&self.target, EditTarget::Existing(window) if !window.append && &window.revision == revision)
    }

    /// Exact packed original for a replace-only source-read delivery.
    /// Stripped BOM bytes are not delivered; append and add remain outside this initial binding.
    pub(crate) fn matches_exact_replacement_page(&self, page: &AgentSourcePage) -> bool {
        matches!(&self.target, EditTarget::Existing(window)
            if !window.append && &window.revision == page.revision()
                && window.range.start == page.range().start_byte() as usize
                && window.range.end == page.range().end_byte() as usize
                && window.exact == page.text())
    }

    pub(crate) fn source_text(&self) -> &str {
        match &self.target {
            EditTarget::Existing(window) => &window.exact,
            EditTarget::Add(_) => "",
        }
    }

    pub(crate) async fn revalidate(
        &self,
        current: EditPhaseState<'_>,
        patch_tool: &dyn WorkspacePatchTool,
        control: &dyn WorkspacePatchControl,
    ) -> Result<(), EditPhaseFailure> {
        if current.anchor() != self.anchor {
            return Err(EditPhaseFailure::InactiveOrStale);
        }
        if let EditTarget::Existing(window) = &self.target {
            let base = read_window_base(window, current, patch_tool, control).await?;
            existing_line_endings(window, &base)?;
        }
        Ok(())
    }

    /// Consumes the binding after decoding one reply. It returns a proposal, with no authority to apply.
    pub async fn complete(
        self,
        reply: BoundEditPhaseReply<'_>,
        current: EditPhaseState<'_>,
        patch_tool: &dyn WorkspacePatchTool,
        control: &dyn WorkspacePatchControl,
    ) -> Result<EditPhaseOutcome, EditPhaseFailure> {
        self.complete_inner(reply, current, None, patch_tool, control)
            .await
    }

    pub(crate) async fn complete_request(
        self,
        reply: BoundEditPhaseReply<'_>,
        current: EditPhaseState<'_>,
        compiled: &CompiledEditPhaseRequest,
        patch_tool: &dyn WorkspacePatchTool,
        control: &dyn WorkspacePatchControl,
    ) -> Result<EditPhaseOutcome, EditPhaseFailure> {
        self.complete_inner(reply, current, Some(compiled), patch_tool, control)
            .await
    }

    async fn complete_inner(
        self,
        reply: BoundEditPhaseReply<'_>,
        current: EditPhaseState<'_>,
        compiled: Option<&CompiledEditPhaseRequest>,
        patch_tool: &dyn WorkspacePatchTool,
        control: &dyn WorkspacePatchControl,
    ) -> Result<EditPhaseOutcome, EditPhaseFailure> {
        self.require_current_scope(current)?;
        if let Some(compiled) = compiled {
            self.require_scope_binding(compiled)?;
            if reply.request != Some(compiled.request_digest())
                || reply.frame != self.scope_frame_digest()
            {
                return Err(EditPhaseFailure::WrongReplyBinding);
            }
        } else if !matches!(self.scope, EditScopeBinding::Unqualified) {
            return Err(EditPhaseFailure::ScopeFrameUnproven);
        }
        if let Some(clock) = self.clock {
            clock.check(control)?;
        }
        if current.anchor != self.anchor {
            return Err(EditPhaseFailure::InactiveOrStale);
        }
        if control.is_cancelled() {
            return Err(EditPhaseFailure::Cancelled);
        }
        if a3_domain::SecretCandidateClassifierV1::classify(self.source_text()).is_some() {
            return Err(EditPhaseFailure::InvalidWindow);
        }
        let replacement = match reply.decode(self.job_digest)? {
            DecodedEditReply::Replace(replacement) => replacement,
            DecodedEditReply::NeedEvidence(need) => {
                return Ok(EditPhaseOutcome::NeedEvidence(need));
            }
            DecodedEditReply::TooLarge => return Ok(EditPhaseOutcome::TooLarge),
        };
        let operation = match self.target {
            EditTarget::Existing(window) => {
                let base = read_window_base(&window, current, patch_tool, control).await?;
                if let Some(clock) = self.clock {
                    clock.check(control)?;
                }
                let operation = compile_existing(window, &base, &replacement)?;
                if let EditScopeBinding::Qualified(frame) = &self.scope {
                    let adapter = self
                        .language
                        .as_deref()
                        .ok_or(EditPhaseFailure::ScopeFrameUnproven)?;
                    let clock = self.clock.ok_or(EditPhaseFailure::ScopeFrameUnproven)?;
                    let PatchOperation::Update(update) = &operation else {
                        return Err(EditPhaseFailure::ScopeFrameUnproven);
                    };
                    frame.verify(
                        current,
                        base.as_bytes().len(),
                        update.content().as_bytes(),
                        adapter,
                        clock,
                        control,
                    )?;
                }
                operation
            }
            EditTarget::Add(path) => {
                if replacement.is_empty() {
                    return Err(EditPhaseFailure::NoChange);
                }
                let content = PatchFileContent::try_from_bytes(replacement.into_bytes())
                    .map_err(|_| EditPhaseFailure::InvalidContent)?;
                PatchOperation::Add(PatchAdd::new(path, content))
            }
        };
        if control.is_cancelled() {
            return Err(EditPhaseFailure::Cancelled);
        }
        if let Some(clock) = self.clock {
            clock.check(control)?;
        }
        let rationale =
            PatchRationale::try_from_string("Apply the active step's exact edit".to_owned())
                .map_err(|_| EditPhaseFailure::InvalidPatch)?;
        let action = PatchAction::new(
            PatchActionSchemaVersion::V1,
            self.anchor.run,
            self.anchor.worktree,
            self.anchor.snapshot,
            self.anchor.step,
            self.anchor.verification,
            rationale,
            vec![operation],
        )
        .map_err(|_| EditPhaseFailure::InvalidPatch)?;
        Ok(EditPhaseOutcome::Proposal(AgentAction::ApplyPatch(
            Box::new(action),
        )))
    }
}

async fn read_window_base(
    window: &ExistingWindow,
    current: EditPhaseState<'_>,
    patch_tool: &dyn WorkspacePatchTool,
    control: &dyn WorkspacePatchControl,
) -> Result<PatchFileContent, EditPhaseFailure> {
    patch_tool
        .read_update_base(
            current.project(),
            current.published,
            &window.revision,
            control,
        )
        .await
        .map_err(EditPhaseFailure::ReadBase)
}

impl fmt::Debug for PreparedEditPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedEditPhase")
            .field("anchor", &self.anchor)
            .field("input_bytes", &self.input.len())
            .finish_non_exhaustive()
    }
}

#[derive(Serialize)]
struct EditInput<'a> {
    version: u8,
    operation: &'a str,
    current: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    inventory: Option<&'static str>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditReply {
    code: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditNeedReply {
    need: EditEvidenceNeed,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditSizeReply {
    #[serde(rename = "tooLarge")]
    too_large: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum EditReplyDocument {
    Edit(EditReply),
    Need(EditNeedReply),
    Size(EditSizeReply),
}

#[derive(Debug, PartialEq, Eq)]
enum DecodedEditReply {
    Replace(String),
    NeedEvidence(EditEvidenceNeed),
    TooLarge,
}

/// Closed evidence gap relative to the single Core-selected window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditEvidenceNeed {
    /// The relevant implementation or definition is missing.
    Definition,
    /// An affected caller is missing.
    Caller,
    /// An applicable test source is missing.
    Test,
    /// A syntactically valid but unsupported request; no raw kind is retained.
    #[serde(other)]
    Unknown,
}

/// A valid edit-phase reply. Evidence and size outcomes do not authorize a mutation.
#[derive(Debug, PartialEq, Eq)]
pub enum EditPhaseOutcome {
    /// Exact existing patch action compiled by the Core.
    Proposal(AgentAction),
    /// More current evidence is needed before proposing a change.
    NeedEvidence(EditEvidenceNeed),
    /// The proposed scope needs a smaller phase.
    TooLarge,
}

fn decode_reply(bytes: &[u8]) -> Result<DecodedEditReply, EditPhaseFailure> {
    if bytes.len() > MAX_EDIT_PHASE_REPLY_BYTES {
        return Err(EditPhaseFailure::TooLarge);
    }
    if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
        return Err(EditPhaseFailure::InvalidReply);
    }
    let reply: EditReplyDocument =
        serde_json::from_slice(bytes).map_err(|_| EditPhaseFailure::InvalidReply)?;
    let mut reply = match reply {
        EditReplyDocument::Edit(reply) => DecodedEditReply::Replace(reply.code),
        EditReplyDocument::Need(reply) => DecodedEditReply::NeedEvidence(reply.need),
        EditReplyDocument::Size(reply) if reply.too_large => DecodedEditReply::TooLarge,
        EditReplyDocument::Size(_) => return Err(EditPhaseFailure::InvalidReply),
    };
    let DecodedEditReply::Replace(replacement) = &mut reply else {
        return Ok(reply);
    };
    if replacement.len() > MAX_EDIT_BYTES {
        return Err(EditPhaseFailure::TooLarge);
    }
    // The model may echo the supplied CRLF representation. Keep the internal form LF,
    // without letting normalization shrink an oversized raw replacement past the byte limit.
    if replacement.contains('\r') {
        *replacement = replacement.replace("\r\n", "\n");
    }
    if replacement
        .chars()
        .any(|value| value == '\u{feff}' || (value.is_control() && !matches!(value, '\n' | '\t')))
    {
        return Err(EditPhaseFailure::InvalidReplacement);
    }
    Ok(reply)
}

fn compile_existing(
    window: ExistingWindow,
    base: &PatchFileContent,
    replacement: &str,
) -> Result<PatchOperation, EditPhaseFailure> {
    let eol = existing_line_endings(&window, base)?;
    let bytes = base.as_bytes();
    let insert = if window.append {
        window.range.end
    } else {
        window.range.start
    };
    let end = window.range.end;
    // Preserve the known complete-line boundary inside an existing file. This separator
    // belongs to Core EOL handling after the raw model-code limit, never to source search.
    let starts_line = window.range.start == 0
        || bytes.get(window.range.start.saturating_sub(1)) == Some(&b'\n')
        || (base.encoding() == PatchTextEncoding::Utf8Bom && window.range.start == 3);
    let replacement = if !window.append
        && !replacement.is_empty()
        && !replacement.ends_with('\n')
        && starts_line
        && window.exact.ends_with('\n')
        && end < bytes.len()
    {
        std::borrow::Cow::Owned(format!("{replacement}\n"))
    } else {
        std::borrow::Cow::Borrowed(replacement)
    };
    let replacement = if eol == PatchLineEndings::Crlf {
        std::borrow::Cow::Owned(replacement.replace('\n', "\r\n"))
    } else {
        replacement
    };
    let mut next = Vec::with_capacity(insert + replacement.len() + bytes.len() - end);
    next.extend_from_slice(&bytes[..insert]);
    next.extend_from_slice(replacement.as_bytes());
    next.extend_from_slice(&bytes[end..]);
    let next =
        PatchFileContent::try_from_bytes(next).map_err(|_| EditPhaseFailure::InvalidContent)?;
    if next.encoding() != base.encoding() {
        return Err(EditPhaseFailure::InvalidContent);
    }
    let update = PatchUpdate::new(window.revision, next).map_err(|error| match error {
        PatchOperationError::NoContentChange => EditPhaseFailure::NoChange,
        PatchOperationError::SameMovePath => EditPhaseFailure::InvalidPatch,
    })?;
    Ok(PatchOperation::Update(update))
}

fn existing_line_endings(
    window: &ExistingWindow,
    base: &PatchFileContent,
) -> Result<PatchLineEndings, EditPhaseFailure> {
    let bytes = base.as_bytes();
    if base.content_hash() != window.revision.content_hash()
        || bytes.get(window.range.clone()) != Some(window.exact.as_bytes())
        || (base.encoding() == PatchTextEncoding::Utf8Bom && window.range.start < 3)
        || (window.append && window.range.end != bytes.len())
        || cuts_crlf(bytes, window.range.start)
        || cuts_crlf(bytes, window.range.end)
    {
        return Err(EditPhaseFailure::InvalidWindow);
    }
    edit_eol(&window.exact, base.line_endings())
}

fn cuts_crlf(bytes: &[u8], offset: usize) -> bool {
    offset > 0 && bytes.get(offset - 1..=offset) == Some(b"\r\n")
}

fn edit_eol(text: &str, fallback: PatchLineEndings) -> Result<PatchLineEndings, EditPhaseFailure> {
    let mut eol = None;
    let mut bytes = text.bytes().peekable();
    while let Some(byte) = bytes.next() {
        let found = match byte {
            b'\n' => PatchLineEndings::Lf,
            b'\r' if bytes.next_if_eq(&b'\n').is_some() => PatchLineEndings::Crlf,
            b'\r' => return Err(EditPhaseFailure::AmbiguousLineEndings),
            _ => continue,
        };
        if eol.is_some_and(|previous| previous != found) {
            return Err(EditPhaseFailure::AmbiguousLineEndings);
        }
        eol = Some(found);
    }
    match eol.unwrap_or(fallback) {
        PatchLineEndings::None | PatchLineEndings::Lf => Ok(PatchLineEndings::Lf),
        PatchLineEndings::Crlf => Ok(PatchLineEndings::Crlf),
        PatchLineEndings::Cr | PatchLineEndings::Mixed => {
            Err(EditPhaseFailure::AmbiguousLineEndings)
        }
    }
}

/// Closed candidate edit failures. Diagnostics contain no supplied code or reply bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditPhaseFailure {
    /// The current execution anchor does not own this job.
    InactiveOrStale,
    /// Source geometry or the exact delivered window does not match the live revision.
    InvalidWindow,
    /// Window or output crossed its fixed bound.
    TooLarge,
    /// Output did not contain exactly one versioned edit reply.
    InvalidReply,
    /// A Core-bound reply belonged to another exact edit job.
    WrongReplyBinding,
    /// The proposed text used unsupported control characters or a BOM.
    InvalidReplacement,
    /// Source line endings do not support a deterministic single conversion.
    AmbiguousLineEndings,
    /// The candidate edit has no effect.
    NoChange,
    /// Secure reread failed or detected a live conflict.
    ReadBase(PatchPreviewFailure),
    /// The assembled full file did not meet existing patch content constraints.
    InvalidContent,
    /// The assembled proposal was rejected by the existing PatchAction constructor.
    InvalidPatch,
    /// A qualified candidate changed an unchanged outside declaration's parents.
    ScopeFrameMismatch,
    /// A mandatory current complete-callable frame or its candidate proof is missing.
    ScopeFrameUnproven,
    /// The unchanged original run deadline expired, including qualification or settlement.
    DeadlineReached,
    /// The owning task cancelled the operation.
    Cancelled,
}

impl fmt::Display for EditPhaseFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InactiveOrStale => "edit phase execution anchor is stale",
            Self::InvalidWindow => "edit window does not match its current source",
            Self::TooLarge => "edit phase crossed its byte limit",
            Self::InvalidReply => "edit phase reply is invalid",
            Self::WrongReplyBinding => "edit phase reply belongs to another job",
            Self::InvalidReplacement => "edit phase replacement contains unsupported characters",
            Self::AmbiguousLineEndings => "edit window has ambiguous line endings",
            Self::NoChange => "edit phase proposal has no effect",
            Self::ReadBase(_) => "edit phase could not reread the current update base",
            Self::InvalidContent => "edit phase produced invalid patch content",
            Self::InvalidPatch => "edit phase produced an invalid patch proposal",
            Self::ScopeFrameMismatch => "edit changed an outside declaration parent",
            Self::ScopeFrameUnproven => "edit declaration parent frame is unproven",
            Self::DeadlineReached => "edit original run deadline was reached",
            Self::Cancelled => "edit phase was cancelled",
        })
    }
}

impl Error for EditPhaseFailure {}

#[cfg(test)]
mod tests;
