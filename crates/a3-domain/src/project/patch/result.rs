use super::{
    PatchAction, PatchActionDigest, PatchFileContent, PatchLineEndings, PatchOperation,
    PatchTextEncoding,
};
use crate::{
    AgentRunId, ContentHash, FileRevision, PolicyDecisionId, RepositoryPath, SnapshotId,
    TaskStepId, VerificationSpecId, WorktreeId,
};
use std::error::Error;
use std::fmt;

const MAX_PATCH_CONTENT_PREVIEW_BYTES: usize = 16 * 1_024;
const MAX_PATCH_PREVIEW_BYTES: usize = 64 * 1_024;

/// Bounded exact prefix plus full-content metadata for one side of a patch preview.
#[derive(Clone, PartialEq, Eq)]
pub struct PatchContentPreview {
    bytes: Vec<u8>,
    total_bytes: u64,
    content_hash: ContentHash,
    encoding: PatchTextEncoding,
    line_endings: PatchLineEndings,
    truncated: bool,
}

impl PatchContentPreview {
    /// Creates a secret-safe preview retaining at most the caller's bounded share.
    pub fn from_content(
        content: &PatchFileContent,
        maximum_bytes: usize,
    ) -> Result<Self, PatchPreviewError> {
        if maximum_bytes > MAX_PATCH_CONTENT_PREVIEW_BYTES {
            return Err(PatchPreviewError::PreviewTooLarge);
        }
        let retained = utf8_prefix(content.as_bytes(), maximum_bytes);
        Ok(Self {
            bytes: retained.to_vec(),
            total_bytes: u64::try_from(content.as_bytes().len())
                .map_err(|_| PatchPreviewError::PreviewTooLarge)?,
            content_hash: content.content_hash(),
            encoding: content.encoding(),
            line_endings: content.line_endings(),
            truncated: retained.len() < content.as_bytes().len(),
        })
    }

    /// Returns exact unnormalized retained bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the full untruncated byte count.
    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Returns the full-content hash, not a prefix hash.
    #[must_use]
    pub const fn content_hash(&self) -> ContentHash {
        self.content_hash
    }

    /// Returns full-content encoding classification.
    #[must_use]
    pub const fn encoding(&self) -> PatchTextEncoding {
        self.encoding
    }

    /// Returns full-content line-ending classification.
    #[must_use]
    pub const fn line_endings(&self) -> PatchLineEndings {
        self.line_endings
    }

    /// Returns whether the retained prefix omits a tail.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

impl fmt::Debug for PatchContentPreview {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PatchContentPreview")
            .field("retained_bytes", &self.bytes.len())
            .field("total_bytes", &self.total_bytes)
            .field("content_hash", &self.content_hash)
            .field("encoding", &self.encoding)
            .field("line_endings", &self.line_endings)
            .field("truncated", &self.truncated)
            .finish()
    }
}

/// One structured before/after preview entry corresponding to one operation.
#[derive(Clone, PartialEq, Eq)]
pub struct PatchPreviewEntry {
    source_path: Option<RepositoryPath>,
    target_path: Option<RepositoryPath>,
    before: Option<PatchContentPreview>,
    after: Option<PatchContentPreview>,
}

impl PatchPreviewEntry {
    /// Creates one entry; the enclosing preview validates it against the action operation.
    #[must_use]
    pub const fn new(
        source_path: Option<RepositoryPath>,
        target_path: Option<RepositoryPath>,
        before: Option<PatchContentPreview>,
        after: Option<PatchContentPreview>,
    ) -> Self {
        Self {
            source_path,
            target_path,
            before,
            after,
        }
    }

    /// Returns the existing path for Update, Move, or Delete.
    #[must_use]
    pub const fn source_path(&self) -> Option<&RepositoryPath> {
        self.source_path.as_ref()
    }

    /// Returns the resulting path for Add, Update, or Move.
    #[must_use]
    pub const fn target_path(&self) -> Option<&RepositoryPath> {
        self.target_path.as_ref()
    }

    /// Returns the bounded exact prior-content prefix.
    #[must_use]
    pub const fn before(&self) -> Option<&PatchContentPreview> {
        self.before.as_ref()
    }

    /// Returns the bounded exact resulting-content prefix.
    #[must_use]
    pub const fn after(&self) -> Option<&PatchContentPreview> {
        self.after.as_ref()
    }
}

impl fmt::Debug for PatchPreviewEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PatchPreviewEntry")
            .field("has_source", &self.source_path.is_some())
            .field("has_target", &self.target_path.is_some())
            .field("before", &self.before)
            .field("after", &self.after)
            .finish()
    }
}

/// Complete bounded preview for one immutable action before approval is requested.
#[derive(Clone, PartialEq, Eq)]
pub struct PatchPreview {
    action_digest: PatchActionDigest,
    snapshot_id: SnapshotId,
    entries: Vec<PatchPreviewEntry>,
    retained_bytes: usize,
}

impl PatchPreview {
    /// Validates exact operation correspondence and the global 64-KiB preview boundary.
    pub fn new(
        action: &PatchAction,
        entries: Vec<PatchPreviewEntry>,
    ) -> Result<Self, PatchPreviewError> {
        if entries.len() != action.operations().len() {
            return Err(PatchPreviewError::OperationMismatch);
        }
        let retained_bytes = entries.iter().try_fold(0usize, |total, entry| {
            let bytes = entry.before().map_or(0, |preview| preview.bytes().len())
                + entry.after().map_or(0, |preview| preview.bytes().len());
            total
                .checked_add(bytes)
                .ok_or(PatchPreviewError::PreviewTooLarge)
        })?;
        if retained_bytes > MAX_PATCH_PREVIEW_BYTES
            || action
                .operations()
                .iter()
                .zip(&entries)
                .any(|(operation, entry)| !entry_matches(operation, entry))
        {
            return Err(if retained_bytes > MAX_PATCH_PREVIEW_BYTES {
                PatchPreviewError::PreviewTooLarge
            } else {
                PatchPreviewError::OperationMismatch
            });
        }
        Ok(Self {
            action_digest: action.digest(),
            snapshot_id: action.snapshot_id(),
            entries,
            retained_bytes,
        })
    }

    /// Returns the exact action being previewed.
    #[must_use]
    pub const fn action_digest(&self) -> PatchActionDigest {
        self.action_digest
    }

    /// Returns the base snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns entries in canonical action order.
    #[must_use]
    pub fn entries(&self) -> &[PatchPreviewEntry] {
        &self.entries
    }

    /// Returns exact retained preview bytes across both sides.
    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
}

impl fmt::Debug for PatchPreview {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PatchPreview")
            .field("action_digest", &self.action_digest)
            .field("snapshot_id", &self.snapshot_id)
            .field("entry_count", &self.entries.len())
            .field("retained_bytes", &self.retained_bytes)
            .finish()
    }
}

/// Invalid or unbounded structured patch preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchPreviewError {
    /// Preview entry shape or hash did not match its operation.
    OperationMismatch,
    /// A single or aggregate preview crossed its fixed boundary.
    PreviewTooLarge,
}

impl fmt::Display for PatchPreviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OperationMismatch => "patch preview does not match the action",
            Self::PreviewTooLarge => "patch preview exceeds its fixed boundary",
        })
    }
}

impl Error for PatchPreviewError {}

/// One actual filesystem transition after successful patch application.
#[derive(Clone, PartialEq, Eq)]
pub enum PatchChange {
    /// An absent path became a current revision.
    Added(FileRevision),
    /// One exact revision was replaced by another at the same path.
    Updated {
        /// Revision invalidated by the write.
        previous: FileRevision,
        /// Actual resulting full-content revision.
        current: FileRevision,
    },
    /// One exact revision moved without content change.
    Moved {
        /// Revision invalidated at the source path.
        previous: FileRevision,
        /// Actual revision at the destination path.
        current: FileRevision,
    },
    /// One exact revision was removed.
    Deleted(FileRevision),
}

impl PatchChange {
    /// Returns resulting evidence when content remains present.
    #[must_use]
    pub const fn current_revision(&self) -> Option<&FileRevision> {
        match self {
            Self::Added(current) | Self::Updated { current, .. } | Self::Moved { current, .. } => {
                Some(current)
            }
            Self::Deleted(_) => None,
        }
    }

    /// Returns invalidated evidence when content existed before the patch.
    #[must_use]
    pub const fn previous_revision(&self) -> Option<&FileRevision> {
        match self {
            Self::Added(_) => None,
            Self::Updated { previous, .. }
            | Self::Moved { previous, .. }
            | Self::Deleted(previous) => Some(previous),
        }
    }

    /// Returns every path hint requiring immediate evidence invalidation and re-observation.
    #[must_use]
    pub fn changed_paths(&self) -> Vec<&RepositoryPath> {
        match self {
            Self::Added(current) => vec![current.path()],
            Self::Updated { current, .. } => vec![current.path()],
            Self::Moved { previous, current } => vec![previous.path(), current.path()],
            Self::Deleted(previous) => vec![previous.path()],
        }
    }
}

impl fmt::Debug for PatchChange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Added(_) => "PatchChange::Added([REDACTED])",
            Self::Updated { .. } => "PatchChange::Updated([REDACTED])",
            Self::Moved { .. } => "PatchChange::Moved([REDACTED])",
            Self::Deleted(_) => "PatchChange::Deleted([REDACTED])",
        })
    }
}

/// Actual post-patch change set bound to approval, task, verification, and base snapshot.
#[derive(Clone, PartialEq, Eq)]
pub struct PatchChangeSet {
    action_digest: PatchActionDigest,
    policy_decision_id: PolicyDecisionId,
    run_id: AgentRunId,
    worktree_id: WorktreeId,
    base_snapshot_id: SnapshotId,
    task_step_id: TaskStepId,
    verification_spec_id: VerificationSpecId,
    changes: Vec<PatchChange>,
    complete: bool,
}

impl PatchChangeSet {
    /// Accepts only exact actual transitions matching every canonical operation.
    pub fn new(
        action: &PatchAction,
        policy_decision_id: PolicyDecisionId,
        changes: Vec<PatchChange>,
    ) -> Result<Self, PatchChangeSetError> {
        Self::build(action, policy_decision_id, changes, true)
    }

    /// Records a non-empty canonical prefix when a later independently atomic operation failed.
    pub fn partial(
        action: &PatchAction,
        policy_decision_id: PolicyDecisionId,
        changes: Vec<PatchChange>,
    ) -> Result<Self, PatchChangeSetError> {
        Self::build(action, policy_decision_id, changes, false)
    }

    fn build(
        action: &PatchAction,
        policy_decision_id: PolicyDecisionId,
        changes: Vec<PatchChange>,
        complete: bool,
    ) -> Result<Self, PatchChangeSetError> {
        let valid_count = if complete {
            changes.len() == action.operations().len()
        } else {
            !changes.is_empty() && changes.len() < action.operations().len()
        };
        if !valid_count
            || action
                .operations()
                .iter()
                .zip(&changes)
                .any(|(operation, change)| !change_matches(operation, change))
        {
            return Err(PatchChangeSetError::OperationMismatch);
        }
        Ok(Self {
            action_digest: action.digest(),
            policy_decision_id,
            run_id: action.run_id(),
            worktree_id: action.worktree_id(),
            base_snapshot_id: action.snapshot_id(),
            task_step_id: action.task_step_id(),
            verification_spec_id: action.verification_spec_id(),
            changes,
            complete,
        })
    }

    /// Returns the exact applied action.
    #[must_use]
    pub const fn action_digest(&self) -> PatchActionDigest {
        self.action_digest
    }

    /// Returns the central allowed decision that authorized mutation.
    #[must_use]
    pub const fn policy_decision_id(&self) -> PolicyDecisionId {
        self.policy_decision_id
    }

    /// Returns the owning run.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the mutated worktree.
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    /// Returns the snapshot that was revalidated before mutation.
    #[must_use]
    pub const fn base_snapshot_id(&self) -> SnapshotId {
        self.base_snapshot_id
    }

    /// Returns the owning task step.
    #[must_use]
    pub const fn task_step_id(&self) -> TaskStepId {
        self.task_step_id
    }

    /// Returns the required post-patch verification.
    #[must_use]
    pub const fn verification_spec_id(&self) -> VerificationSpecId {
        self.verification_spec_id
    }

    /// Returns actual transitions in canonical action order.
    #[must_use]
    pub fn changes(&self) -> &[PatchChange] {
        &self.changes
    }

    /// Returns whether every authorized operation completed.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.complete
    }

    /// Returns sorted unique hints that must invalidate stale evidence before further reasoning.
    #[must_use]
    pub fn changed_paths(&self) -> Vec<RepositoryPath> {
        let mut paths = self
            .changes
            .iter()
            .flat_map(PatchChange::changed_paths)
            .cloned()
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        paths
    }
}

impl fmt::Debug for PatchChangeSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PatchChangeSet")
            .field("action_digest", &self.action_digest)
            .field("policy_decision_id", &self.policy_decision_id)
            .field("run_id", &self.run_id)
            .field("worktree_id", &self.worktree_id)
            .field("base_snapshot_id", &self.base_snapshot_id)
            .field("task_step_id", &self.task_step_id)
            .field("verification_spec_id", &self.verification_spec_id)
            .field("change_count", &self.changes.len())
            .field("complete", &self.complete)
            .finish()
    }
}

/// Actual change-set evidence disagreed with the authorized action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchChangeSetError {
    /// Count, paths, or hashes did not exactly match an operation.
    OperationMismatch,
}

impl fmt::Display for PatchChangeSetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("patch change set does not match the authorized action")
    }
}

impl Error for PatchChangeSetError {}

fn entry_matches(operation: &PatchOperation, entry: &PatchPreviewEntry) -> bool {
    match operation {
        PatchOperation::Add(add) => {
            entry.source_path().is_none()
                && entry.target_path() == Some(add.path())
                && entry.before().is_none()
                && entry
                    .after()
                    .is_some_and(|after| after.content_hash() == add.content().content_hash())
        }
        PatchOperation::Update(update) => {
            entry.source_path() == Some(update.expected().path())
                && entry.target_path() == Some(update.expected().path())
                && entry
                    .before()
                    .is_some_and(|before| before.content_hash() == update.expected().content_hash())
                && entry
                    .after()
                    .is_some_and(|after| after.content_hash() == update.content().content_hash())
        }
        PatchOperation::Move(movement) => {
            entry.source_path() == Some(movement.expected().path())
                && entry.target_path() == Some(movement.destination())
                && entry.before().is_some_and(|before| {
                    before.content_hash() == movement.expected().content_hash()
                })
                && entry
                    .after()
                    .is_some_and(|after| after.content_hash() == movement.expected().content_hash())
        }
        PatchOperation::Delete(expected) => {
            entry.source_path() == Some(expected.path())
                && entry.target_path().is_none()
                && entry
                    .before()
                    .is_some_and(|before| before.content_hash() == expected.content_hash())
                && entry.after().is_none()
        }
    }
}

fn change_matches(operation: &PatchOperation, change: &PatchChange) -> bool {
    match (operation, change) {
        (PatchOperation::Add(add), PatchChange::Added(current)) => {
            current.path() == add.path() && current.content_hash() == add.content().content_hash()
        }
        (PatchOperation::Update(update), PatchChange::Updated { previous, current }) => {
            previous == update.expected()
                && current.path() == update.expected().path()
                && current.content_hash() == update.content().content_hash()
        }
        (PatchOperation::Move(movement), PatchChange::Moved { previous, current }) => {
            previous == movement.expected()
                && current.path() == movement.destination()
                && current.content_hash() == movement.expected().content_hash()
        }
        (PatchOperation::Delete(expected), PatchChange::Deleted(previous)) => previous == expected,
        _ => false,
    }
}

fn utf8_prefix(bytes: &[u8], maximum: usize) -> &[u8] {
    let mut end = bytes.len().min(maximum);
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() {
        end = end.saturating_sub(1);
    }
    &bytes[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AgentRunId, PatchActionSchemaVersion, PatchAdd, PatchRationale, TaskStepId,
        VerificationSpecId, WorktreeId,
    };

    #[test]
    fn preview_and_change_set_are_exactly_action_bound() -> Result<(), Box<dyn Error>> {
        let path = RepositoryPath::try_from_bytes(b"src/new.rs".to_vec())?;
        let content = PatchFileContent::try_from_bytes(b"pub fn added() {}\n".to_vec())?;
        let action = PatchAction::new(
            PatchActionSchemaVersion::V1,
            AgentRunId::from_bytes([1; 32]),
            WorktreeId::from_bytes([2; 32]),
            SnapshotId::from_bytes([3; 32]),
            TaskStepId::from_bytes([4; 32]),
            VerificationSpecId::from_bytes([5; 32]),
            PatchRationale::try_from_string("add verified source".to_owned())?,
            vec![PatchOperation::Add(PatchAdd::new(
                path.clone(),
                content.clone(),
            ))],
        )?;
        let after = PatchContentPreview::from_content(&content, 16 * 1_024)?;
        let preview = PatchPreview::new(
            &action,
            vec![PatchPreviewEntry::new(
                None,
                Some(path.clone()),
                None,
                Some(after),
            )],
        )?;
        assert_eq!(
            preview.entries()[0].after().map(PatchContentPreview::bytes),
            Some(content.as_bytes())
        );

        let current = FileRevision::new(path, content.content_hash());
        let changes = PatchChangeSet::new(
            &action,
            PolicyDecisionId::from_bytes([6; 32]),
            vec![PatchChange::Added(current.clone())],
        )?;
        assert_eq!(changes.changes()[0].current_revision(), Some(&current));
        assert_eq!(changes.changed_paths(), vec![current.path().clone()]);
        Ok(())
    }
}
