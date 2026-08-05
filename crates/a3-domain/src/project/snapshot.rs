use super::{GitHead, SnapshotId, WorktreeId};
use std::error::Error;
use std::fmt;

const MAX_REPOSITORY_PATH_BYTES: usize = 131_072;
const MAX_ADAPTER_VERSION_BYTES: usize = 128;
const MAX_PERSISTED_SEQUENCE: u64 = i64::MAX as u64;

/// Cryptographic BLAKE3 digest of one observed file revision.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Constructs a content hash from a verified 256-bit digest.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical binary representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ContentHash(")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        formatter.write_str(")")
    }
}

/// Canonical Git-style repository-relative path represented without platform loss.
///
/// Components are separated by `/`. Absolute paths, empty components, traversal,
/// and NUL are rejected while non-UTF-8 bytes remain representable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepositoryPath(Vec<u8>);

impl RepositoryPath {
    /// Validates normalized repository-relative path bytes.
    pub fn try_from_bytes(bytes: Vec<u8>) -> Result<Self, RepositoryPathError> {
        if bytes.is_empty() || bytes.len() > MAX_REPOSITORY_PATH_BYTES {
            return Err(RepositoryPathError::InvalidLength(bytes.len()));
        }
        if bytes.first() == Some(&b'/') || bytes.last() == Some(&b'/') {
            return Err(RepositoryPathError::NotRelativeNormalized);
        }
        if bytes.contains(&0) {
            return Err(RepositoryPathError::NulByte);
        }
        if bytes
            .split(|byte| *byte == b'/')
            .any(|component| component.is_empty() || component == b"." || component == b"..")
        {
            return Err(RepositoryPathError::NotRelativeNormalized);
        }
        Ok(Self(bytes))
    }

    /// Returns the canonical repository-relative byte representation.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Invalid repository-relative path supplied by discovery or reconstructed from storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryPathError {
    /// The path was empty or exceeded the fixed storage and context boundary.
    InvalidLength(usize),
    /// The path was absolute, had empty components, or contained traversal components.
    NotRelativeNormalized,
    /// Git path bytes contained NUL, which cannot name a repository entry.
    NulByte,
}

impl fmt::Display for RepositoryPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => {
                write!(
                    formatter,
                    "repository path has invalid byte length {length}"
                )
            }
            Self::NotRelativeNormalized => {
                formatter.write_str("repository path is not normalized and relative")
            }
            Self::NulByte => formatter.write_str("repository path contains NUL"),
        }
    }
}

impl Error for RepositoryPathError {}

/// Monotone generation of an observed worktree state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorktreeGeneration(u64);

impl WorktreeGeneration {
    /// Creates a positive generation that fits the durable integer representation.
    pub fn new(value: u64) -> Result<Self, WorktreeGenerationError> {
        if value == 0 {
            return Err(WorktreeGenerationError::Zero);
        }
        if value > MAX_PERSISTED_SEQUENCE {
            return Err(WorktreeGenerationError::TooLarge(value));
        }
        Ok(Self(value))
    }

    /// Returns the durable integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Returns the immediately following generation or an exhaustion error.
    pub fn next(self) -> Result<Self, WorktreeGenerationError> {
        self.0
            .checked_add(1)
            .ok_or(WorktreeGenerationError::Exhausted)
            .and_then(Self::new)
    }
}

/// Invalid or exhausted worktree generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeGenerationError {
    /// Generation zero is not an observed state.
    Zero,
    /// The value cannot be represented by the durable sequence.
    TooLarge(u64),
    /// No later durable generation can be represented.
    Exhausted,
}

impl fmt::Display for WorktreeGenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => formatter.write_str("worktree generation must be positive"),
            Self::TooLarge(value) => {
                write!(formatter, "worktree generation {value} is too large")
            }
            Self::Exhausted => formatter.write_str("worktree generation is exhausted"),
        }
    }
}

impl Error for WorktreeGenerationError {}

/// Version of the deterministic index schema represented by a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexSchemaVersion(u32);

impl IndexSchemaVersion {
    /// Returns the initial deterministic index schema revision.
    #[must_use]
    pub const fn v1() -> Self {
        Self(1)
    }

    /// Returns the exact-retrieval projection schema revision.
    #[must_use]
    pub const fn v2() -> Self {
        Self(2)
    }

    /// Creates a non-zero index schema version.
    pub fn new(value: u32) -> Result<Self, IndexSchemaVersionError> {
        if value == 0 {
            return Err(IndexSchemaVersionError);
        }
        Ok(Self(value))
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Index schema version zero is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexSchemaVersionError;

impl fmt::Display for IndexSchemaVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("index schema version must be positive")
    }
}

impl Error for IndexSchemaVersionError {}

/// V1 language-adapter family whose revision affects snapshot compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IndexLanguage {
    /// Generic text, manifest, and file classification.
    Generic,
    /// Rust structural adapter.
    Rust,
    /// Shared TypeScript and JavaScript structural adapter.
    TypeScriptJavaScript,
    /// Python structural adapter.
    Python,
}

impl IndexLanguage {
    /// Returns the stable persisted identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Rust => "rust",
            Self::TypeScriptJavaScript => "typescript-javascript",
            Self::Python => "python",
        }
    }

    /// Reconstructs a language identifier from durable storage.
    pub fn try_from_stored(value: &str) -> Result<Self, SnapshotError> {
        match value {
            "generic" => Ok(Self::Generic),
            "rust" => Ok(Self::Rust),
            "typescript-javascript" => Ok(Self::TypeScriptJavaScript),
            "python" => Ok(Self::Python),
            _ => Err(SnapshotError::InvalidStoredLanguage),
        }
    }
}

/// Bounded version identifier of one language adapter and its grammar contract.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LanguageAdapterVersion(String);

impl LanguageAdapterVersion {
    /// Validates a stable ASCII version identifier.
    pub fn try_from_string(value: String) -> Result<Self, LanguageAdapterVersionError> {
        let bytes = value.as_bytes();
        if bytes.is_empty() || bytes.len() > MAX_ADAPTER_VERSION_BYTES {
            return Err(LanguageAdapterVersionError::InvalidLength(bytes.len()));
        }
        if !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+'))
        {
            return Err(LanguageAdapterVersionError::InvalidCharacter);
        }
        Ok(Self(value))
    }

    /// Returns the stable persisted version identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Invalid language-adapter version identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageAdapterVersionError {
    /// The identifier was empty or exceeded its fixed bound.
    InvalidLength(usize),
    /// The identifier contained characters outside the safe stable alphabet.
    InvalidCharacter,
}

impl fmt::Display for LanguageAdapterVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => {
                write!(
                    formatter,
                    "adapter version has invalid byte length {length}"
                )
            }
            Self::InvalidCharacter => {
                formatter.write_str("adapter version contains an invalid character")
            }
        }
    }
}

impl Error for LanguageAdapterVersionError {}

/// Exact language-adapter revision captured by a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LanguageAdapterRevision {
    language: IndexLanguage,
    version: LanguageAdapterVersion,
}

impl LanguageAdapterRevision {
    /// Creates one adapter revision.
    #[must_use]
    pub const fn new(language: IndexLanguage, version: LanguageAdapterVersion) -> Self {
        Self { language, version }
    }

    /// Returns the adapter family.
    #[must_use]
    pub const fn language(&self) -> IndexLanguage {
        self.language
    }

    /// Returns the adapter and grammar version.
    #[must_use]
    pub const fn version(&self) -> &LanguageAdapterVersion {
        &self.version
    }
}

/// Durable meaning of one changed path in an immutable snapshot delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotChangeKind {
    /// The path is present with this content in the new snapshot.
    Upsert,
    /// The path was removed; the hash identifies its prior content for invalidation and rename hints.
    Delete,
}

impl SnapshotChangeKind {
    /// Returns the stable persisted identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Upsert => "upsert",
            Self::Delete => "delete",
        }
    }

    /// Reconstructs a change kind from durable storage.
    pub fn try_from_stored(value: &str) -> Result<Self, SnapshotError> {
        match value {
            "upsert" => Ok(Self::Upsert),
            "delete" => Ok(Self::Delete),
            _ => Err(SnapshotError::InvalidStoredChangeKind),
        }
    }
}

/// One changed repository path and its verified current or prior content hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotChange {
    path: RepositoryPath,
    content_hash: ContentHash,
    kind: SnapshotChangeKind,
}

impl SnapshotChange {
    /// Creates a changed path entry.
    #[must_use]
    pub const fn new(
        path: RepositoryPath,
        content_hash: ContentHash,
        kind: SnapshotChangeKind,
    ) -> Self {
        Self {
            path,
            content_hash,
            kind,
        }
    }

    /// Returns the normalized repository-relative path.
    #[must_use]
    pub const fn path(&self) -> &RepositoryPath {
        &self.path
    }

    /// Returns the verified current or prior content hash.
    #[must_use]
    pub const fn content_hash(&self) -> ContentHash {
        self.content_hash
    }

    /// Returns whether the new state contains or deletes the path.
    #[must_use]
    pub const fn kind(&self) -> SnapshotChangeKind {
        self.kind
    }
}

/// Immutable, deterministically ordered observation of one worktree generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    id: SnapshotId,
    worktree_id: WorktreeId,
    parent_id: Option<SnapshotId>,
    generation: WorktreeGeneration,
    head: GitHead,
    index_schema_version: IndexSchemaVersion,
    adapter_revisions: Vec<LanguageAdapterRevision>,
    changes: Vec<SnapshotChange>,
}

impl Snapshot {
    /// Creates a snapshot while canonicalizing set order and rejecting duplicates.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: SnapshotId,
        worktree_id: WorktreeId,
        parent_id: Option<SnapshotId>,
        generation: WorktreeGeneration,
        head: GitHead,
        index_schema_version: IndexSchemaVersion,
        mut adapter_revisions: Vec<LanguageAdapterRevision>,
        mut changes: Vec<SnapshotChange>,
    ) -> Result<Self, SnapshotError> {
        if (generation.get() == 1) != parent_id.is_none() {
            return Err(SnapshotError::ParentGenerationMismatch);
        }
        if parent_id == Some(id) {
            return Err(SnapshotError::SelfParent);
        }
        if adapter_revisions.is_empty() {
            return Err(SnapshotError::MissingAdapterRevisions);
        }
        adapter_revisions.sort();
        if adapter_revisions
            .windows(2)
            .any(|pair| pair[0].language == pair[1].language)
        {
            return Err(SnapshotError::DuplicateAdapterRevision);
        }
        changes.sort_by(|left, right| left.path.cmp(&right.path));
        if changes.windows(2).any(|pair| pair[0].path == pair[1].path) {
            return Err(SnapshotError::DuplicatePath);
        }
        Ok(Self {
            id,
            worktree_id,
            parent_id,
            generation,
            head,
            index_schema_version,
            adapter_revisions,
            changes,
        })
    }

    /// Returns the immutable snapshot identity.
    #[must_use]
    pub const fn id(&self) -> SnapshotId {
        self.id
    }

    /// Returns the observed worktree identity.
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    /// Returns the immediately preceding snapshot when this is not the first generation.
    #[must_use]
    pub const fn parent_id(&self) -> Option<SnapshotId> {
        self.parent_id
    }

    /// Returns the monotone worktree generation.
    #[must_use]
    pub const fn generation(&self) -> WorktreeGeneration {
        self.generation
    }

    /// Returns the HEAD state captured by this snapshot.
    #[must_use]
    pub const fn head(&self) -> &GitHead {
        &self.head
    }

    /// Returns the deterministic index schema version.
    #[must_use]
    pub const fn index_schema_version(&self) -> IndexSchemaVersion {
        self.index_schema_version
    }

    /// Returns adapter revisions in canonical language order.
    #[must_use]
    pub fn adapter_revisions(&self) -> &[LanguageAdapterRevision] {
        &self.adapter_revisions
    }

    /// Returns changed paths in canonical byte order.
    #[must_use]
    pub fn changes(&self) -> &[SnapshotChange] {
        &self.changes
    }
}

/// Invalid snapshot aggregate or malformed stored snapshot field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotError {
    /// Generation one must have no parent and every later generation must have one.
    ParentGenerationMismatch,
    /// A snapshot referred to itself as its predecessor.
    SelfParent,
    /// Reproducibility requires at least one adapter revision.
    MissingAdapterRevisions,
    /// More than one revision was supplied for one adapter family.
    DuplicateAdapterRevision,
    /// More than one change targeted the same normalized path.
    DuplicatePath,
    /// Durable storage contained an unknown language identifier.
    InvalidStoredLanguage,
    /// Durable storage contained an unknown change-kind identifier.
    InvalidStoredChangeKind,
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParentGenerationMismatch => {
                formatter.write_str("snapshot parent does not match its generation")
            }
            Self::SelfParent => formatter.write_str("snapshot cannot be its own parent"),
            Self::MissingAdapterRevisions => {
                formatter.write_str("snapshot requires adapter revisions")
            }
            Self::DuplicateAdapterRevision => {
                formatter.write_str("snapshot contains duplicate adapter revisions")
            }
            Self::DuplicatePath => formatter.write_str("snapshot contains duplicate paths"),
            Self::InvalidStoredLanguage => {
                formatter.write_str("stored snapshot language is invalid")
            }
            Self::InvalidStoredChangeKind => {
                formatter.write_str("stored snapshot change kind is invalid")
            }
        }
    }
}

impl Error for SnapshotError {}

#[cfg(test)]
mod tests {
    use super::{
        ContentHash, IndexLanguage, IndexSchemaVersion, LanguageAdapterRevision,
        LanguageAdapterVersion, RepositoryPath, Snapshot, SnapshotChange, SnapshotChangeKind,
        SnapshotError, WorktreeGeneration,
    };
    use crate::{GitHead, GitReferenceName, SnapshotId, WorktreeId};

    #[test]
    fn repository_path_rejects_absolute_traversal_and_empty_components() {
        for bytes in [
            b"/root".to_vec(),
            b"a/../b".to_vec(),
            b"a//b".to_vec(),
            b"a/.".to_vec(),
        ] {
            assert!(RepositoryPath::try_from_bytes(bytes).is_err());
        }
        assert!(RepositoryPath::try_from_bytes(vec![b'f', 0, b'o']).is_err());
    }

    #[test]
    fn repository_path_retains_non_utf8_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let path = RepositoryPath::try_from_bytes(vec![b's', b'r', b'c', b'/', 0xff])?;
        assert_eq!(path.as_bytes(), &[b's', b'r', b'c', b'/', 0xff]);
        Ok(())
    }

    #[test]
    fn snapshot_canonicalizes_sets_and_rejects_duplicates() -> Result<(), Box<dyn std::error::Error>>
    {
        let rust = adapter(IndexLanguage::Rust, "1.0.0")?;
        let generic = adapter(IndexLanguage::Generic, "2.0.0")?;
        let second_path = RepositoryPath::try_from_bytes(b"src/z.rs".to_vec())?;
        let first_path = RepositoryPath::try_from_bytes(b"src/a.rs".to_vec())?;
        let snapshot = Snapshot::new(
            SnapshotId::from_bytes([1; 32]),
            WorktreeId::from_bytes([2; 32]),
            None,
            WorktreeGeneration::new(1)?,
            unborn_head()?,
            IndexSchemaVersion::new(1)?,
            vec![rust.clone(), generic],
            vec![
                SnapshotChange::new(
                    second_path,
                    ContentHash::from_bytes([4; 32]),
                    SnapshotChangeKind::Upsert,
                ),
                SnapshotChange::new(
                    first_path.clone(),
                    ContentHash::from_bytes([3; 32]),
                    SnapshotChangeKind::Upsert,
                ),
            ],
        )?;

        assert_eq!(
            snapshot.adapter_revisions()[0].language(),
            IndexLanguage::Generic
        );
        assert_eq!(snapshot.changes()[0].path(), &first_path);
        assert_eq!(
            Snapshot::new(
                SnapshotId::from_bytes([5; 32]),
                WorktreeId::from_bytes([2; 32]),
                None,
                WorktreeGeneration::new(1)?,
                unborn_head()?,
                IndexSchemaVersion::new(1)?,
                vec![rust.clone(), rust],
                Vec::new(),
            ),
            Err(SnapshotError::DuplicateAdapterRevision)
        );
        assert_eq!(
            Snapshot::new(
                SnapshotId::from_bytes([6; 32]),
                WorktreeId::from_bytes([2; 32]),
                None,
                WorktreeGeneration::new(2)?,
                unborn_head()?,
                IndexSchemaVersion::new(1)?,
                vec![adapter(IndexLanguage::Generic, "1")?],
                Vec::new(),
            ),
            Err(SnapshotError::ParentGenerationMismatch)
        );
        Ok(())
    }

    fn adapter(
        language: IndexLanguage,
        version: &str,
    ) -> Result<LanguageAdapterRevision, Box<dyn std::error::Error>> {
        Ok(LanguageAdapterRevision::new(
            language,
            LanguageAdapterVersion::try_from_string(version.to_owned())?,
        ))
    }

    fn unborn_head() -> Result<GitHead, Box<dyn std::error::Error>> {
        Ok(GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        })
    }
}
