use crate::{
    AgentRunId, ContentHash, DiscoveryExclusionReason, DiscoveryPolicy, FileRevision,
    RepositoryPath, SecretCandidateClassifierV1, SnapshotId, TaskStepId, VerificationSpecId,
    WorktreeId,
};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

const MAX_PATCH_OPERATIONS: usize = 64;
const MAX_PATCH_FILE_BYTES: usize = 4 * 1_024 * 1_024;
const MAX_PATCH_TOTAL_CONTENT_BYTES: usize = 16 * 1_024 * 1_024;
const MAX_PATCH_RATIONALE_BYTES: usize = 4 * 1_024;

/// Version of the closed structured patch-action contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PatchActionSchemaVersion {
    /// Initial full-file Add, Update, Move, and Delete contract.
    V1,
}

/// Exact text encoding admitted by the V1 patch boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PatchTextEncoding {
    /// UTF-8 without a byte-order mark.
    Utf8,
    /// UTF-8 beginning with the exact three-byte byte-order mark.
    Utf8Bom,
}

/// Exact line-ending shape observed without normalizing patch bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PatchLineEndings {
    /// No line ending occurs in the content.
    None,
    /// Every line ending is LF.
    Lf,
    /// Every line ending is CRLF.
    Crlf,
    /// Every line ending is bare CR.
    Cr,
    /// More than one line-ending representation occurs.
    Mixed,
}

/// Secret-safe, text-only, bounded complete content for an Add or Update operation.
#[derive(Clone, PartialEq, Eq)]
pub struct PatchFileContent {
    bytes: Vec<u8>,
    content_hash: ContentHash,
    encoding: PatchTextEncoding,
    line_endings: PatchLineEndings,
}

impl PatchFileContent {
    /// Validates full UTF-8 bytes without changing BOM, line endings, or code points.
    pub fn try_from_bytes(bytes: Vec<u8>) -> Result<Self, PatchFileContentError> {
        if bytes.len() > MAX_PATCH_FILE_BYTES {
            return Err(PatchFileContentError::TooLarge {
                actual: bytes.len(),
            });
        }
        let policy = DiscoveryPolicy::v1();
        let prefix_length = bytes.len().min(policy.inspection_prefix_bytes());
        if let Some(reason) = policy.classify_content_prefix(&bytes[..prefix_length]) {
            return Err(match reason {
                DiscoveryExclusionReason::Binary => PatchFileContentError::Binary,
                DiscoveryExclusionReason::Secret => PatchFileContentError::SecretCandidate,
                _ => PatchFileContentError::InvalidContent,
            });
        }
        let text =
            std::str::from_utf8(&bytes).map_err(|_| PatchFileContentError::InvalidEncoding)?;
        if SecretCandidateClassifierV1::classify(text).is_some() {
            return Err(PatchFileContentError::SecretCandidate);
        }
        let encoding = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            PatchTextEncoding::Utf8Bom
        } else {
            PatchTextEncoding::Utf8
        };
        let line_endings = classify_line_endings(&bytes);
        let content_hash = ContentHash::from_bytes(*blake3::hash(&bytes).as_bytes());
        Ok(Self {
            bytes,
            content_hash,
            encoding,
            line_endings,
        })
    }

    /// Returns the exact bytes that must be written.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the exact full-content digest.
    #[must_use]
    pub const fn content_hash(&self) -> ContentHash {
        self.content_hash
    }

    /// Returns the exact admitted UTF-8 representation.
    #[must_use]
    pub const fn encoding(&self) -> PatchTextEncoding {
        self.encoding
    }

    /// Returns the line-ending shape without normalizing it.
    #[must_use]
    pub const fn line_endings(&self) -> PatchLineEndings {
        self.line_endings
    }
}

impl fmt::Debug for PatchFileContent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PatchFileContent")
            .field("bytes", &self.bytes.len())
            .field("content_hash", &self.content_hash)
            .field("encoding", &self.encoding)
            .field("line_endings", &self.line_endings)
            .finish()
    }
}

/// Patch content crossed a fixed security or resource boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchFileContentError {
    /// Full content exceeded four MiB.
    TooLarge {
        /// Observed byte count.
        actual: usize,
    },
    /// Full content was not valid UTF-8.
    InvalidEncoding,
    /// Bounded classification identified binary content.
    Binary,
    /// Secret classification stopped content before it became executable input.
    SecretCandidate,
    /// A classifier returned a category invalid for content-only input.
    InvalidContent,
}

impl fmt::Display for PatchFileContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooLarge { .. } => "patch file content exceeds the four MiB boundary",
            Self::InvalidEncoding => "patch file content is not valid UTF-8",
            Self::Binary => "patch file content is binary",
            Self::SecretCandidate => "patch file content contains a possible secret",
            Self::InvalidContent => "patch file content classification is invalid",
        })
    }
}

impl Error for PatchFileContentError {}

/// Bounded human rationale retained with a patch but redacted from debug output.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PatchRationale(String);

impl PatchRationale {
    /// Normalizes newlines and rejects empty, control-containing, or oversized text.
    pub fn try_from_string(value: String) -> Result<Self, PatchRationaleError> {
        let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
        let trimmed = normalized.trim();
        if trimmed.is_empty() || trimmed.len() > MAX_PATCH_RATIONALE_BYTES {
            return Err(PatchRationaleError::InvalidLength {
                actual: trimmed.len(),
            });
        }
        if trimmed.chars().any(|character| {
            character == '\0' || (character.is_control() && !matches!(character, '\n' | '\t'))
        }) {
            return Err(PatchRationaleError::InvalidCharacter);
        }
        if SecretCandidateClassifierV1::classify(trimmed).is_some() {
            return Err(PatchRationaleError::SecretCandidate);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// Returns the normalized rationale.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PatchRationale {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PatchRationale")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Invalid patch rationale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchRationaleError {
    /// Text was empty or exceeded the fixed bound.
    InvalidLength {
        /// Normalized observed bytes.
        actual: usize,
    },
    /// Text contained an unsupported control character.
    InvalidCharacter,
    /// Text contained a possible secret.
    SecretCandidate,
}

impl fmt::Display for PatchRationaleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLength { .. } => "patch rationale length is invalid",
            Self::InvalidCharacter => "patch rationale contains an unsupported character",
            Self::SecretCandidate => "patch rationale contains a possible secret",
        })
    }
}

impl Error for PatchRationaleError {}

/// Add one previously absent regular text file.
#[derive(Clone, PartialEq, Eq)]
pub struct PatchAdd {
    path: RepositoryPath,
    content: PatchFileContent,
}

impl PatchAdd {
    /// Creates an Add operation; absence is revalidated by the workspace adapter.
    #[must_use]
    pub const fn new(path: RepositoryPath, content: PatchFileContent) -> Self {
        Self { path, content }
    }

    /// Returns the exact new path.
    #[must_use]
    pub const fn path(&self) -> &RepositoryPath {
        &self.path
    }

    /// Returns exact bytes and their digest.
    #[must_use]
    pub const fn content(&self) -> &PatchFileContent {
        &self.content
    }
}

impl fmt::Debug for PatchAdd {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PatchAdd([REDACTED])")
    }
}

/// Replace one exact current file revision with complete new text bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct PatchUpdate {
    expected: FileRevision,
    content: PatchFileContent,
}

impl PatchUpdate {
    /// Rejects a content-identical update.
    pub fn new(
        expected: FileRevision,
        content: PatchFileContent,
    ) -> Result<Self, PatchOperationError> {
        if expected.content_hash() == content.content_hash() {
            return Err(PatchOperationError::NoContentChange);
        }
        Ok(Self { expected, content })
    }

    /// Returns the exact source revision that must still exist.
    #[must_use]
    pub const fn expected(&self) -> &FileRevision {
        &self.expected
    }

    /// Returns exact replacement bytes and their digest.
    #[must_use]
    pub const fn content(&self) -> &PatchFileContent {
        &self.content
    }
}

impl fmt::Debug for PatchUpdate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PatchUpdate([REDACTED])")
    }
}

/// Move one exact current file revision to a previously absent path without changing bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct PatchMove {
    expected: FileRevision,
    destination: RepositoryPath,
}

impl PatchMove {
    /// Rejects a move whose source and destination are identical.
    pub fn new(
        expected: FileRevision,
        destination: RepositoryPath,
    ) -> Result<Self, PatchOperationError> {
        if expected.path() == &destination {
            return Err(PatchOperationError::SameMovePath);
        }
        Ok(Self {
            expected,
            destination,
        })
    }

    /// Returns the exact source revision that must still exist.
    #[must_use]
    pub const fn expected(&self) -> &FileRevision {
        &self.expected
    }

    /// Returns the absent destination path.
    #[must_use]
    pub const fn destination(&self) -> &RepositoryPath {
        &self.destination
    }
}

impl fmt::Debug for PatchMove {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PatchMove([REDACTED])")
    }
}

/// Invalid operation-local relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchOperationError {
    /// Update bytes have the expected existing hash.
    NoContentChange,
    /// Move source and destination are the same path.
    SameMovePath,
}

impl fmt::Display for PatchOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoContentChange => "patch update does not change content",
            Self::SameMovePath => "patch move source and destination are identical",
        })
    }
}

impl Error for PatchOperationError {}

/// Closed full-file mutation operation.
#[derive(Clone, PartialEq, Eq)]
pub enum PatchOperation {
    /// Add an absent path.
    Add(PatchAdd),
    /// Replace an exact current revision.
    Update(PatchUpdate),
    /// Move an exact current revision to an absent path.
    Move(PatchMove),
    /// Delete an exact current revision.
    Delete(FileRevision),
}

impl PatchOperation {
    /// Returns the canonical primary path used to order independent operations.
    #[must_use]
    pub const fn primary_path(&self) -> &RepositoryPath {
        match self {
            Self::Add(add) => add.path(),
            Self::Update(update) => update.expected().path(),
            Self::Move(movement) => movement.expected().path(),
            Self::Delete(expected) => expected.path(),
        }
    }

    /// Returns whether this operation irreversibly removes content.
    #[must_use]
    pub const fn is_destructive(&self) -> bool {
        matches!(self, Self::Delete(_))
    }

    /// Returns complete new content when bytes are supplied by the action.
    #[must_use]
    pub const fn new_content(&self) -> Option<&PatchFileContent> {
        match self {
            Self::Add(add) => Some(add.content()),
            Self::Update(update) => Some(update.content()),
            Self::Move(_) | Self::Delete(_) => None,
        }
    }

    pub(crate) fn touched_paths(&self) -> impl Iterator<Item = &RepositoryPath> {
        let second = match self {
            Self::Move(movement) => Some(movement.destination()),
            _ => None,
        };
        std::iter::once(self.primary_path()).chain(second)
    }

    pub(crate) const fn kind_tag(&self) -> u8 {
        match self {
            Self::Add(_) => 0,
            Self::Update(_) => 1,
            Self::Move(_) => 2,
            Self::Delete(_) => 3,
        }
    }
}

impl fmt::Debug for PatchOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Add(_) => "PatchOperation::Add([REDACTED])",
            Self::Update(_) => "PatchOperation::Update([REDACTED])",
            Self::Move(_) => "PatchOperation::Move([REDACTED])",
            Self::Delete(_) => "PatchOperation::Delete([REDACTED])",
        })
    }
}

/// Digest of exact patch semantics including all expected and replacement content hashes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PatchActionDigest([u8; 32]);

impl PatchActionDigest {
    /// Reconstructs a digest after its enclosing persisted action evidence is revalidated.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the stable digest bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for PatchActionDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PatchActionDigest([REDACTED])")
    }
}

/// Digest of the exact worktree path set covered by a patch approval.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PatchScopeDigest([u8; 32]);

impl PatchScopeDigest {
    /// Returns the stable digest bytes.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for PatchScopeDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PatchScopeDigest([REDACTED])")
    }
}

/// Content-free policy projection binding approval to one exact complete patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PatchPolicyAction {
    worktree_id: WorktreeId,
    action_digest: PatchActionDigest,
    scope_digest: PatchScopeDigest,
    destructive: bool,
}

impl PatchPolicyAction {
    /// Returns the exact worktree.
    #[must_use]
    pub const fn worktree_id(self) -> WorktreeId {
        self.worktree_id
    }

    /// Returns the exact patch-semantics digest.
    #[must_use]
    pub const fn action_digest(self) -> PatchActionDigest {
        self.action_digest
    }

    /// Returns the covered path-set digest.
    #[must_use]
    pub const fn scope_digest(self) -> PatchScopeDigest {
        self.scope_digest
    }

    /// Returns whether the patch contains a Delete operation.
    #[must_use]
    pub const fn destructive(self) -> bool {
        self.destructive
    }
}

/// Immutable snapshot-, run-, step-, and verification-bound structured patch.
#[derive(Clone, PartialEq, Eq)]
pub struct PatchAction {
    version: PatchActionSchemaVersion,
    run_id: AgentRunId,
    worktree_id: WorktreeId,
    snapshot_id: SnapshotId,
    task_step_id: TaskStepId,
    verification_spec_id: VerificationSpecId,
    rationale: PatchRationale,
    operations: Vec<PatchOperation>,
    digest: PatchActionDigest,
    scope_digest: PatchScopeDigest,
}

impl PatchAction {
    /// Canonicalizes independent operations and rejects overlapping or unsafe path scopes.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: PatchActionSchemaVersion,
        run_id: AgentRunId,
        worktree_id: WorktreeId,
        snapshot_id: SnapshotId,
        task_step_id: TaskStepId,
        verification_spec_id: VerificationSpecId,
        rationale: PatchRationale,
        mut operations: Vec<PatchOperation>,
    ) -> Result<Self, PatchActionError> {
        if operations.is_empty() || operations.len() > MAX_PATCH_OPERATIONS {
            return Err(PatchActionError::InvalidOperationCount {
                actual: operations.len(),
            });
        }
        operations.sort_by(|left, right| {
            left.primary_path()
                .cmp(right.primary_path())
                .then_with(|| left.kind_tag().cmp(&right.kind_tag()))
        });
        let mut touched = BTreeSet::new();
        let mut total_content_bytes = 0usize;
        let policy = DiscoveryPolicy::v1();
        for operation in &operations {
            for path in operation.touched_paths() {
                if !touched.insert(path.clone()) {
                    return Err(PatchActionError::OverlappingPath);
                }
                if policy
                    .classify_built_in_path(path.as_bytes(), false)
                    .is_some()
                {
                    return Err(PatchActionError::ForbiddenPath);
                }
            }
            if let Some(content) = operation.new_content() {
                total_content_bytes = total_content_bytes
                    .checked_add(content.as_bytes().len())
                    .ok_or(PatchActionError::TooMuchContent)?;
            }
        }
        if total_content_bytes > MAX_PATCH_TOTAL_CONTENT_BYTES {
            return Err(PatchActionError::TooMuchContent);
        }
        let digest = derive_action_digest(
            version,
            run_id,
            worktree_id,
            snapshot_id,
            task_step_id,
            verification_spec_id,
            &rationale,
            &operations,
        );
        let scope_digest = derive_scope_digest(worktree_id, &operations);
        Ok(Self {
            version,
            run_id,
            worktree_id,
            snapshot_id,
            task_step_id,
            verification_spec_id,
            rationale,
            operations,
            digest,
            scope_digest,
        })
    }

    /// Returns the structured schema version.
    #[must_use]
    pub const fn version(&self) -> PatchActionSchemaVersion {
        self.version
    }

    /// Returns the owning agent run.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the exact selected worktree.
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    /// Returns the immutable snapshot expected before application.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the owning task step.
    #[must_use]
    pub const fn task_step_id(&self) -> TaskStepId {
        self.task_step_id
    }

    /// Returns the verification that must run after the change set is observed.
    #[must_use]
    pub const fn verification_spec_id(&self) -> VerificationSpecId {
        self.verification_spec_id
    }

    /// Returns the bounded rationale.
    #[must_use]
    pub const fn rationale(&self) -> &PatchRationale {
        &self.rationale
    }

    /// Returns canonical independent operations.
    #[must_use]
    pub fn operations(&self) -> &[PatchOperation] {
        &self.operations
    }

    /// Returns the exact action digest.
    #[must_use]
    pub const fn digest(&self) -> PatchActionDigest {
        self.digest
    }

    /// Returns the one content-free central-policy action required before application.
    #[must_use]
    pub fn policy_action(&self) -> crate::PolicyAction {
        crate::PolicyAction::Patch(PatchPolicyAction {
            worktree_id: self.worktree_id,
            action_digest: self.digest,
            scope_digest: self.scope_digest,
            destructive: self.operations.iter().any(PatchOperation::is_destructive),
        })
    }
}

impl fmt::Debug for PatchAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PatchAction")
            .field("version", &self.version)
            .field("run_id", &self.run_id)
            .field("worktree_id", &self.worktree_id)
            .field("snapshot_id", &self.snapshot_id)
            .field("task_step_id", &self.task_step_id)
            .field("verification_spec_id", &self.verification_spec_id)
            .field("operation_count", &self.operations.len())
            .field("digest", &self.digest)
            .finish()
    }
}

/// Patch-action invariants rejected unsafe or ambiguous mutation semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchActionError {
    /// Operation count was zero or above 64.
    InvalidOperationCount {
        /// Observed operation count.
        actual: usize,
    },
    /// More than one operation touched the same path.
    OverlappingPath,
    /// A path belongs to a non-overridable secret, binary, generated, or vendor class.
    ForbiddenPath,
    /// Aggregate replacement content exceeded sixteen MiB.
    TooMuchContent,
}

impl fmt::Display for PatchActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidOperationCount { .. } => "patch operation count is outside 1..=64",
            Self::OverlappingPath => "patch operations overlap on one repository path",
            Self::ForbiddenPath => "patch targets a forbidden repository path",
            Self::TooMuchContent => "patch replacement content exceeds the aggregate boundary",
        })
    }
}

impl Error for PatchActionError {}

fn classify_line_endings(bytes: &[u8]) -> PatchLineEndings {
    let mut lf = false;
    let mut crlf = false;
    let mut cr = false;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index.saturating_add(1)) == Some(&b'\n') => {
                crlf = true;
                index = index.saturating_add(2);
            }
            b'\r' => {
                cr = true;
                index = index.saturating_add(1);
            }
            b'\n' => {
                lf = true;
                index = index.saturating_add(1);
            }
            _ => index = index.saturating_add(1),
        }
    }
    match (lf, crlf, cr) {
        (false, false, false) => PatchLineEndings::None,
        (true, false, false) => PatchLineEndings::Lf,
        (false, true, false) => PatchLineEndings::Crlf,
        (false, false, true) => PatchLineEndings::Cr,
        _ => PatchLineEndings::Mixed,
    }
}

#[allow(clippy::too_many_arguments)]
fn derive_action_digest(
    version: PatchActionSchemaVersion,
    run_id: AgentRunId,
    worktree_id: WorktreeId,
    snapshot_id: SnapshotId,
    task_step_id: TaskStepId,
    verification_spec_id: VerificationSpecId,
    rationale: &PatchRationale,
    operations: &[PatchOperation],
) -> PatchActionDigest {
    let mut hasher = blake3::Hasher::new_derive_key("a3.patch-action.v1");
    hasher.update(&[match version {
        PatchActionSchemaVersion::V1 => 1,
    }]);
    hasher.update(run_id.as_bytes());
    hasher.update(worktree_id.as_bytes());
    hasher.update(snapshot_id.as_bytes());
    hasher.update(task_step_id.as_bytes());
    hasher.update(verification_spec_id.as_bytes());
    hash_bytes(&mut hasher, rationale.as_str().as_bytes());
    for operation in operations {
        hasher.update(&[operation.kind_tag()]);
        hash_bytes(&mut hasher, operation.primary_path().as_bytes());
        match operation {
            PatchOperation::Add(add) => {
                hasher.update(add.content().content_hash().as_bytes());
            }
            PatchOperation::Update(update) => {
                hasher.update(update.expected().content_hash().as_bytes());
                hasher.update(update.content().content_hash().as_bytes());
            }
            PatchOperation::Move(movement) => {
                hasher.update(movement.expected().content_hash().as_bytes());
                hash_bytes(&mut hasher, movement.destination().as_bytes());
            }
            PatchOperation::Delete(expected) => {
                hasher.update(expected.content_hash().as_bytes());
            }
        }
    }
    PatchActionDigest(*hasher.finalize().as_bytes())
}

fn derive_scope_digest(worktree_id: WorktreeId, operations: &[PatchOperation]) -> PatchScopeDigest {
    let mut hasher = blake3::Hasher::new_derive_key("a3.patch-scope.v1");
    hasher.update(worktree_id.as_bytes());
    for operation in operations {
        hasher.update(&[operation.kind_tag()]);
        for path in operation.touched_paths() {
            hash_bytes(&mut hasher, path.as_bytes());
        }
    }
    PatchScopeDigest(*hasher.finalize().as_bytes())
}

fn hash_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_preserves_bom_crlf_and_non_ascii_bytes() -> Result<(), Box<dyn Error>> {
        let bytes = b"\xef\xbb\xbfGr\xc3\xbc\xc3\x9fe\r\nzweite Zeile\r\n".to_vec();
        let content = PatchFileContent::try_from_bytes(bytes.clone())?;
        assert_eq!(content.as_bytes(), bytes);
        assert_eq!(content.encoding(), PatchTextEncoding::Utf8Bom);
        assert_eq!(content.line_endings(), PatchLineEndings::Crlf);
        Ok(())
    }

    #[test]
    fn patch_content_rejects_binary_and_secret_candidates() {
        assert_eq!(
            PatchFileContent::try_from_bytes(b"text\0binary".to_vec()),
            Err(PatchFileContentError::Binary)
        );
        assert_eq!(
            PatchFileContent::try_from_bytes(b"password=fixture-secret-value\n".to_vec()),
            Err(PatchFileContentError::SecretCandidate)
        );
    }

    #[test]
    fn patch_is_canonical_policy_bound_and_rejects_overlap() -> Result<(), Box<dyn Error>> {
        let add = PatchOperation::Add(PatchAdd::new(
            path(b"src/b.rs")?,
            PatchFileContent::try_from_bytes(b"pub fn b() {}\n".to_vec())?,
        ));
        let update = PatchOperation::Update(PatchUpdate::new(
            revision(b"src/a.rs", b"old\n")?,
            PatchFileContent::try_from_bytes(b"new\n".to_vec())?,
        )?);
        let action = patch(vec![add, update])?;
        assert_eq!(
            action.operations()[0].primary_path().as_bytes(),
            b"src/a.rs"
        );
        assert_eq!(action.policy_action().class(), crate::ActionClass::Write);
        assert_eq!(action.policy_action().risk(), crate::RiskLevel::Moderate);
        assert_eq!(
            crate::SystemPolicyV1.disposition(&action.policy_action()),
            crate::PolicyDisposition::ApprovalRequired
        );

        let changed_content = patch(vec![
            PatchOperation::Update(PatchUpdate::new(
                revision(b"src/a.rs", b"old\n")?,
                PatchFileContent::try_from_bytes(b"newer\n".to_vec())?,
            )?),
            action.operations()[1].clone(),
        ])?;
        assert_ne!(
            action.policy_action().fingerprint(),
            changed_content.policy_action().fingerprint()
        );
        assert_eq!(
            action.policy_action().scope_digest(),
            changed_content.policy_action().scope_digest()
        );

        let duplicate = PatchOperation::Delete(revision(b"src/a.rs", b"old\n")?);
        assert_eq!(
            patch(vec![action.operations()[0].clone(), duplicate]),
            Err(PatchActionError::OverlappingPath)
        );
        Ok(())
    }

    fn patch(operations: Vec<PatchOperation>) -> Result<PatchAction, PatchActionError> {
        PatchAction::new(
            PatchActionSchemaVersion::V1,
            AgentRunId::from_bytes([1; 32]),
            WorktreeId::from_bytes([2; 32]),
            SnapshotId::from_bytes([3; 32]),
            TaskStepId::from_bytes([4; 32]),
            VerificationSpecId::from_bytes([5; 32]),
            PatchRationale::try_from_string("apply the verified change".to_owned())
                .map_err(|_| PatchActionError::TooMuchContent)?,
            operations,
        )
    }

    fn path(bytes: &[u8]) -> Result<RepositoryPath, Box<dyn Error>> {
        Ok(RepositoryPath::try_from_bytes(bytes.to_vec())?)
    }

    fn revision(path_bytes: &[u8], content: &[u8]) -> Result<FileRevision, Box<dyn Error>> {
        Ok(FileRevision::new(
            path(path_bytes)?,
            ContentHash::from_bytes(*blake3::hash(content).as_bytes()),
        ))
    }
}
