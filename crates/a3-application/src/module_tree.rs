use crate::{JobContext, KnowledgeStoreFailure};
use a3_domain::{
    FileRevision, IndexRunId, ModuleId, ModuleKind, ModuleRoot, Progress, ProjectIdentity,
    SnapshotId,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const MAX_PAGE_SIZE: u16 = 100;
const DEFAULT_PAGE_SIZE: u16 = 50;
const MAX_DISPLAY_CHARACTERS: usize = 256;

/// Positive bounded number of direct module children returned by one read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleTreePageSize(u16);

impl ModuleTreePageSize {
    /// Product default chosen for progressive desktop rendering.
    pub const DEFAULT: Self = Self(DEFAULT_PAGE_SIZE);

    /// Accepts one through one hundred direct module children per page.
    pub fn new(value: u16) -> Result<Self, ModuleTreePageSizeError> {
        if value == 0 || value > MAX_PAGE_SIZE {
            return Err(ModuleTreePageSizeError);
        }
        Ok(Self(value))
    }

    /// Returns the validated SQL and IPC limit.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// A module-tree page size was zero or exceeded the fixed UI bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleTreePageSizeError;

impl fmt::Display for ModuleTreePageSizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module-tree page size must be between one and one hundred")
    }
}

impl Error for ModuleTreePageSizeError {}

/// Bounded sanitized module label that is never accepted as an authoritative root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleTreeDisplayName {
    value: String,
    truncated: bool,
}

impl ModuleTreeDisplayName {
    pub(crate) fn from_root(root: &ModuleRoot) -> Self {
        let bytes = match root {
            ModuleRoot::Repository => return Self::repository(),
            ModuleRoot::Directory(path) => path
                .as_bytes()
                .rsplit(|byte| *byte == b'/')
                .next()
                .unwrap_or(path.as_bytes()),
        };
        let source = String::from_utf8_lossy(bytes);
        let mut characters = source.chars();
        let value = characters
            .by_ref()
            .take(MAX_DISPLAY_CHARACTERS)
            .map(|character| {
                if character.is_control() {
                    '\u{fffd}'
                } else {
                    character
                }
            })
            .collect();
        Self {
            value,
            truncated: characters.next().is_some(),
        }
    }

    fn repository() -> Self {
        Self {
            value: "Repository".to_owned(),
            truncated: false,
        }
    }

    /// Returns safe UI text derived from, but not interchangeable with, the module root.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Returns whether the display omitted characters beyond the fixed bound.
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }
}

/// Whether a primary module has nested primary module boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleTreeChildState {
    /// No nested primary boundary exists in the current projection.
    Leaf,
    /// At least one nested primary boundary exists in the current projection.
    HasChildren,
}

/// Deterministic boundary kinds that are valid nodes in the navigable module tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleTreeEntryKind {
    /// A repository or directory boundary established by one or more manifests.
    ManifestBoundary,
    /// A repository or directory boundary established by deterministic path structure.
    PathBoundary,
}

/// Current deterministic evidence supporting one primary module boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleTreeBoundaryEvidence {
    representative_revision: Option<FileRevision>,
    manifest_revision: Option<FileRevision>,
}

impl ModuleTreeBoundaryEvidence {
    /// Keeps representative membership and manifest evidence consistent with the module kind.
    pub fn new(
        kind: ModuleKind,
        symbol_count: u64,
        representative_revision: Option<FileRevision>,
        manifest_revision: Option<FileRevision>,
    ) -> Result<Self, ModuleTreeEntryError> {
        if !kind.is_primary()
            || (symbol_count == 0) != representative_revision.is_none()
            || matches!(kind, ModuleKind::ManifestBoundary) != manifest_revision.is_some()
        {
            return Err(ModuleTreeEntryError);
        }
        Ok(Self {
            representative_revision,
            manifest_revision,
        })
    }

    /// Returns one current member revision when the module contains structural symbols.
    #[must_use]
    pub const fn representative_revision(&self) -> Option<&FileRevision> {
        self.representative_revision.as_ref()
    }

    /// Returns one current package-manifest revision for a manifest boundary.
    #[must_use]
    pub const fn manifest_revision(&self) -> Option<&FileRevision> {
        self.manifest_revision.as_ref()
    }
}

/// One primary module node with exact bounded projection metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleTreeEntry {
    module_id: ModuleId,
    kind: ModuleTreeEntryKind,
    root: ModuleRoot,
    display_name: ModuleTreeDisplayName,
    boundary_evidence: ModuleTreeBoundaryEvidence,
    manifest_count: u64,
    file_count: u64,
    symbol_count: u64,
    central_symbol_count: u64,
    central_symbols_truncated: bool,
    entrypoint_count: u64,
    entrypoints_truncated: bool,
    test_count: u64,
    tests_truncated: bool,
    child_state: ModuleTreeChildState,
}

impl ModuleTreeEntry {
    /// Constructs a primary module entry while rejecting contradictory counts and evidence.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        module_id: ModuleId,
        kind: ModuleKind,
        root: ModuleRoot,
        boundary_evidence: ModuleTreeBoundaryEvidence,
        manifest_count: u64,
        file_count: u64,
        symbol_count: u64,
        central_symbol_count: u64,
        central_symbols_truncated: bool,
        entrypoint_count: u64,
        entrypoints_truncated: bool,
        test_count: u64,
        tests_truncated: bool,
        child_state: ModuleTreeChildState,
    ) -> Result<Self, ModuleTreeEntryError> {
        let entry_kind = match kind {
            ModuleKind::ManifestBoundary if manifest_count > 0 => {
                ModuleTreeEntryKind::ManifestBoundary
            }
            ModuleKind::PathBoundary if manifest_count == 0 => ModuleTreeEntryKind::PathBoundary,
            ModuleKind::ManifestBoundary
            | ModuleKind::PathBoundary
            | ModuleKind::GraphCommunity => return Err(ModuleTreeEntryError),
        };
        let counts_are_valid = file_count <= symbol_count
            && central_symbol_count <= symbol_count
            && entrypoint_count <= symbol_count
            && test_count <= symbol_count
            && (!central_symbols_truncated || central_symbol_count > 0)
            && (!entrypoints_truncated || entrypoint_count > 0)
            && (!tests_truncated || test_count > 0);
        let evidence_matches = boundary_evidence.representative_revision().is_none()
            == (symbol_count == 0)
            && boundary_evidence.manifest_revision().is_some()
                == matches!(kind, ModuleKind::ManifestBoundary);
        if !counts_are_valid || !evidence_matches {
            return Err(ModuleTreeEntryError);
        }
        let display_name = ModuleTreeDisplayName::from_root(&root);
        Ok(Self {
            module_id,
            kind: entry_kind,
            root,
            display_name,
            boundary_evidence,
            manifest_count,
            file_count,
            symbol_count,
            central_symbol_count,
            central_symbols_truncated,
            entrypoint_count,
            entrypoints_truncated,
            test_count,
            tests_truncated,
            child_state,
        })
    }

    /// Returns the stable deterministic module identity.
    #[must_use]
    pub const fn module_id(&self) -> ModuleId {
        self.module_id
    }

    /// Returns the deterministic primary-boundary kind.
    #[must_use]
    pub const fn kind(&self) -> ModuleTreeEntryKind {
        self.kind
    }

    /// Returns the canonical repository or directory root.
    #[must_use]
    pub const fn root(&self) -> &ModuleRoot {
        &self.root
    }

    /// Returns a bounded safe display label.
    #[must_use]
    pub const fn display_name(&self) -> &ModuleTreeDisplayName {
        &self.display_name
    }

    /// Returns current evidence supporting the deterministic boundary.
    #[must_use]
    pub const fn boundary_evidence(&self) -> &ModuleTreeBoundaryEvidence {
        &self.boundary_evidence
    }

    /// Returns the exact number of package manifests at this boundary.
    #[must_use]
    pub const fn manifest_count(&self) -> u64 {
        self.manifest_count
    }

    /// Returns the exact number of distinct current member files.
    #[must_use]
    pub const fn file_count(&self) -> u64 {
        self.file_count
    }

    /// Returns the exact number of primary member symbols.
    #[must_use]
    pub const fn symbol_count(&self) -> u64 {
        self.symbol_count
    }

    /// Returns the stored bounded central-symbol prefix size.
    #[must_use]
    pub const fn central_symbol_count(&self) -> u64 {
        self.central_symbol_count
    }

    /// Returns whether lower-ranked central symbols were omitted.
    #[must_use]
    pub const fn central_symbols_truncated(&self) -> bool {
        self.central_symbols_truncated
    }

    /// Returns the stored bounded entrypoint prefix size.
    #[must_use]
    pub const fn entrypoint_count(&self) -> u64 {
        self.entrypoint_count
    }

    /// Returns whether additional entrypoints were omitted.
    #[must_use]
    pub const fn entrypoints_truncated(&self) -> bool {
        self.entrypoints_truncated
    }

    /// Returns the stored bounded test prefix size.
    #[must_use]
    pub const fn test_count(&self) -> u64 {
        self.test_count
    }

    /// Returns whether additional tests were omitted.
    #[must_use]
    pub const fn tests_truncated(&self) -> bool {
        self.tests_truncated
    }

    /// Returns whether another progressive child page can exist below this node.
    #[must_use]
    pub const fn child_state(&self) -> ModuleTreeChildState {
        self.child_state
    }
}

/// A stored or constructed module-tree entry contradicted projection invariants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleTreeEntryError;

impl fmt::Display for ModuleTreeEntryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module-tree entry shape is invalid")
    }
}

impl Error for ModuleTreeEntryError {}

/// Validated root or direct child-module page request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleTreeQuery {
    parent_module_id: Option<ModuleId>,
    after_module_id: Option<ModuleId>,
    page_size: ModuleTreePageSize,
}

impl ModuleTreeQuery {
    /// Creates a bounded query for top-level or direct nested primary modules.
    #[must_use]
    pub const fn new(
        parent_module_id: Option<ModuleId>,
        after_module_id: Option<ModuleId>,
        page_size: ModuleTreePageSize,
    ) -> Self {
        Self {
            parent_module_id,
            after_module_id,
            page_size,
        }
    }

    /// Returns the parent module, or None for top-level boundaries.
    #[must_use]
    pub const fn parent_module_id(&self) -> Option<ModuleId> {
        self.parent_module_id
    }

    /// Returns the exclusive stable module cursor.
    #[must_use]
    pub const fn after_module_id(&self) -> Option<ModuleId> {
        self.after_module_id
    }

    /// Returns the fixed maximum number of nodes delivered to the UI.
    #[must_use]
    pub const fn page_size(&self) -> ModuleTreePageSize {
        self.page_size
    }
}

/// One canonical bounded page from an atomic deterministic module projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleTreePage {
    index_run_id: IndexRunId,
    snapshot_id: SnapshotId,
    parent_module_id: Option<ModuleId>,
    primary_module_count: u64,
    graph_community_count: u64,
    entries: Vec<ModuleTreeEntry>,
    next_cursor: Option<ModuleId>,
}

impl ModuleTreePage {
    /// Validates stable order, counts, bounds, and next-cursor shape.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index_run_id: IndexRunId,
        snapshot_id: SnapshotId,
        parent_module_id: Option<ModuleId>,
        primary_module_count: u64,
        graph_community_count: u64,
        entries: Vec<ModuleTreeEntry>,
        next_cursor: Option<ModuleId>,
    ) -> Result<Self, ModuleTreePageError> {
        let entry_count =
            u64::try_from(entries.len()).map_err(|_| ModuleTreePageError::InvalidCountsOrBounds)?;
        if entries.len() > usize::from(MAX_PAGE_SIZE) || primary_module_count < entry_count {
            return Err(ModuleTreePageError::InvalidCountsOrBounds);
        }
        if parent_module_id
            .is_some_and(|parent| entries.iter().any(|entry| entry.module_id() == parent))
        {
            return Err(ModuleTreePageError::InvalidHierarchy);
        }
        if entries
            .windows(2)
            .any(|pair| pair[0].module_id() >= pair[1].module_id())
        {
            return Err(ModuleTreePageError::UnstableEntryOrder);
        }
        match (entries.last(), next_cursor) {
            (Some(last), Some(cursor)) if last.module_id() == cursor => {}
            (_, None) => {}
            _ => return Err(ModuleTreePageError::InvalidNextCursor),
        }
        Ok(Self {
            index_run_id,
            snapshot_id,
            parent_module_id,
            primary_module_count,
            graph_community_count,
            entries,
            next_cursor,
        })
    }

    /// Returns the exact published index run behind the page.
    #[must_use]
    pub const fn index_run_id(&self) -> IndexRunId {
        self.index_run_id
    }

    /// Returns the exact immutable snapshot behind the page.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the enumerated parent module, or None for top level.
    #[must_use]
    pub const fn parent_module_id(&self) -> Option<ModuleId> {
        self.parent_module_id
    }

    /// Returns the exact number of primary modules in the projection.
    #[must_use]
    pub const fn primary_module_count(&self) -> u64 {
        self.primary_module_count
    }

    /// Returns the exact number of supplementary graph communities kept outside the tree.
    #[must_use]
    pub const fn graph_community_count(&self) -> u64 {
        self.graph_community_count
    }

    /// Returns at most one hundred direct module children in stable identity order.
    #[must_use]
    pub fn entries(&self) -> &[ModuleTreeEntry] {
        &self.entries
    }

    /// Returns the last delivered module when another page exists.
    #[must_use]
    pub const fn next_cursor(&self) -> Option<ModuleId> {
        self.next_cursor
    }
}

/// Stored rows contradicted the canonical module-tree page contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleTreePageError {
    /// Counts or page size contradicted the projection.
    InvalidCountsOrBounds,
    /// Module IDs were duplicated or not in strict stable order.
    UnstableEntryOrder,
    /// A page admitted its own parent as a direct child.
    InvalidHierarchy,
    /// A next cursor was present without matching the last delivered module.
    InvalidNextCursor,
}

impl fmt::Display for ModuleTreePageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidCountsOrBounds => "module-tree counts or bounds are invalid",
            Self::UnstableEntryOrder => "module-tree entries are not canonically ordered",
            Self::InvalidHierarchy => "module-tree parent and child hierarchy is invalid",
            Self::InvalidNextCursor => "module-tree next cursor is invalid",
        })
    }
}

impl Error for ModuleTreePageError {}

/// Result of reading the latest publication and its optional V8 module projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleTreeLoadResult {
    /// No index has crossed the atomic publish boundary.
    NoPublishedIndex,
    /// The current historical publication predates deterministic module projection.
    ProjectionUnavailable,
    /// One current bounded page is available.
    Page(ModuleTreePage),
}

/// Cooperative cancellation and deterministic progress for one module-tree read.
pub trait ModuleTreeControl: fmt::Debug + Send + Sync {
    /// Returns whether the owning operation cancelled the read.
    fn is_cancelled(&self) -> bool;

    /// Reports the start and completion phases to the owning runtime.
    fn report_progress(&self, progress: Progress) -> Result<(), ModuleTreeControlError>;
}

impl ModuleTreeControl for JobContext {
    fn is_cancelled(&self) -> bool {
        self.cancellation_token().is_cancelled()
    }

    fn report_progress(&self, progress: Progress) -> Result<(), ModuleTreeControlError> {
        JobContext::report_progress(self, progress).map_err(|_| ModuleTreeControlError::Unavailable)
    }
}

/// Module-tree progress could not reach its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleTreeControlError {
    /// The owning runtime no longer accepts progress.
    Unavailable,
}

impl fmt::Display for ModuleTreeControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("module-tree progress is unavailable")
    }
}

impl Error for ModuleTreeControlError {}

/// Owned future returned by the object-safe module-tree port.
pub type ModuleTreeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ModuleTreeLoadResult, ModuleTreeFailure>> + Send + 'a>>;

/// Narrow read-only capability for one published module-tree page.
pub trait ModuleTreeStore: fmt::Debug + Send + Sync {
    /// Loads a bounded page without reconstructing the complete published index.
    fn load_module_tree_page<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a ModuleTreeQuery,
        control: &'a dyn ModuleTreeControl,
    ) -> ModuleTreeFuture<'a>;
}

/// Stable content-free failure classes for module-tree reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleTreeFailure {
    /// Shared local persistence failed.
    Storage(KnowledgeStoreFailure),
    /// Stored rows contradicted the module projection or page contract.
    InvalidStoredProjection,
    /// The selected parent is absent or no longer primary in the current projection.
    ParentUnavailable,
    /// The owner cancelled before a result was delivered.
    Cancelled,
    /// The bounded adapter query exceeded its fixed deadline.
    TimedOut,
    /// Progress delivery failed.
    ProgressUnavailable,
}

impl fmt::Display for ModuleTreeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "module-tree storage failed: {error}"),
            Self::InvalidStoredProjection => {
                formatter.write_str("stored module-tree projection is invalid")
            }
            Self::ParentUnavailable => formatter.write_str("module-tree parent is unavailable"),
            Self::Cancelled => formatter.write_str("module-tree read was cancelled"),
            Self::TimedOut => formatter.write_str("module-tree read timed out"),
            Self::ProgressUnavailable => formatter.write_str("module-tree progress is unavailable"),
        }
    }
}

impl Error for ModuleTreeFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(source) => Some(source),
            Self::InvalidStoredProjection
            | Self::ParentUnavailable
            | Self::Cancelled
            | Self::TimedOut
            | Self::ProgressUnavailable => None,
        }
    }
}

/// Application use case retaining cancellation and progress outside persistence.
#[derive(Debug)]
pub struct GetModuleTreePage {
    store: Arc<dyn ModuleTreeStore>,
}

impl GetModuleTreePage {
    /// Wires the narrow progressive module-tree capability.
    #[must_use]
    pub fn new(store: Arc<dyn ModuleTreeStore>) -> Self {
        Self { store }
    }

    /// Reads one current page or an explicit publication availability state.
    pub async fn execute(
        &self,
        project: &ProjectIdentity,
        query: &ModuleTreeQuery,
        control: &dyn ModuleTreeControl,
    ) -> Result<ModuleTreeLoadResult, ModuleTreeFailure> {
        report(control, 0)?;
        if control.is_cancelled() {
            return Err(ModuleTreeFailure::Cancelled);
        }
        let result = self
            .store
            .load_module_tree_page(project, query, control)
            .await?;
        if control.is_cancelled() {
            return Err(ModuleTreeFailure::Cancelled);
        }
        report(control, 2)?;
        Ok(result)
    }
}

fn report(control: &dyn ModuleTreeControl, completed: u64) -> Result<(), ModuleTreeFailure> {
    control
        .report_progress(
            Progress::determinate(completed, 2)
                .map_err(|_| ModuleTreeFailure::InvalidStoredProjection)?,
        )
        .map_err(|_| ModuleTreeFailure::ProgressUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{
        GetModuleTreePage, ModuleTreeBoundaryEvidence, ModuleTreeChildState, ModuleTreeControl,
        ModuleTreeControlError, ModuleTreeEntry, ModuleTreeFailure, ModuleTreeFuture,
        ModuleTreeLoadResult, ModuleTreePage, ModuleTreePageSize, ModuleTreeQuery, ModuleTreeStore,
    };
    use a3_domain::{
        CanonicalDirectory, ContentHash, FileRevision, GitHead, GitReferenceName, IndexRunId,
        ModuleId, ModuleKind, ModuleRoot, Progress, ProjectIdentity, RepositoryId,
        RepositoryIdentity, RepositoryPath, SnapshotId, WorktreeAnchorId, WorktreeId,
        WorktreeIdentity,
    };
    use futures::executor::block_on;
    use std::error::Error;
    use std::sync::{Arc, Mutex};

    #[derive(Debug)]
    struct RecordingStore;

    impl ModuleTreeStore for RecordingStore {
        fn load_module_tree_page<'a>(
            &'a self,
            _project: &'a ProjectIdentity,
            query: &'a ModuleTreeQuery,
            _control: &'a dyn ModuleTreeControl,
        ) -> ModuleTreeFuture<'a> {
            Box::pin(async move {
                let revision = FileRevision::new(
                    RepositoryPath::try_from_bytes(b"src/lib.rs".to_vec())
                        .map_err(|_| ModuleTreeFailure::InvalidStoredProjection)?,
                    ContentHash::from_bytes([3; 32]),
                );
                let evidence = ModuleTreeBoundaryEvidence::new(
                    ModuleKind::PathBoundary,
                    1,
                    Some(revision),
                    None,
                )
                .map_err(|_| ModuleTreeFailure::InvalidStoredProjection)?;
                let entry = ModuleTreeEntry::new(
                    ModuleId::from_bytes([7; 32]),
                    ModuleKind::PathBoundary,
                    ModuleRoot::Directory(
                        RepositoryPath::try_from_bytes(b"src".to_vec())
                            .map_err(|_| ModuleTreeFailure::InvalidStoredProjection)?,
                    ),
                    evidence,
                    0,
                    1,
                    1,
                    1,
                    false,
                    0,
                    false,
                    0,
                    false,
                    ModuleTreeChildState::Leaf,
                )
                .map_err(|_| ModuleTreeFailure::InvalidStoredProjection)?;
                ModuleTreePage::new(
                    IndexRunId::from_bytes([8; 32]),
                    SnapshotId::from_bytes([9; 32]),
                    query.parent_module_id(),
                    1,
                    2,
                    vec![entry],
                    None,
                )
                .map(ModuleTreeLoadResult::Page)
                .map_err(|_| ModuleTreeFailure::InvalidStoredProjection)
            })
        }
    }

    #[derive(Debug, Default)]
    struct RecordingControl {
        progress: Mutex<Vec<Progress>>,
        cancelled: bool,
    }

    impl ModuleTreeControl for RecordingControl {
        fn is_cancelled(&self) -> bool {
            self.cancelled
        }

        fn report_progress(&self, progress: Progress) -> Result<(), ModuleTreeControlError> {
            self.progress
                .lock()
                .map_err(|_| ModuleTreeControlError::Unavailable)?
                .push(progress);
            Ok(())
        }
    }

    #[test]
    fn entry_rejects_graph_communities_and_contradictory_evidence() -> Result<(), Box<dyn Error>> {
        let member = revision("src/lib.rs", 1)?;
        assert!(
            ModuleTreeBoundaryEvidence::new(
                ModuleKind::GraphCommunity,
                1,
                Some(member.clone()),
                None,
            )
            .is_err()
        );
        let evidence = ModuleTreeBoundaryEvidence::new(
            ModuleKind::ManifestBoundary,
            1,
            Some(member),
            Some(revision("Cargo.toml", 2)?),
        )?;
        assert!(
            ModuleTreeEntry::new(
                ModuleId::from_bytes([3; 32]),
                ModuleKind::ManifestBoundary,
                ModuleRoot::Repository,
                evidence,
                1,
                2,
                1,
                0,
                false,
                0,
                false,
                0,
                false,
                ModuleTreeChildState::Leaf,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn display_is_bounded_while_root_and_evidence_remain_lossless() -> Result<(), Box<dyn Error>> {
        let mut root = vec![b'a'; 300];
        root[2] = b'\n';
        root[3] = 0xff;
        let root_path = RepositoryPath::try_from_bytes(root.clone())?;
        let member = FileRevision::new(
            RepositoryPath::try_from_bytes([root, b"/lib.rs".to_vec()].concat())?,
            ContentHash::from_bytes([4; 32]),
        );
        let entry = ModuleTreeEntry::new(
            ModuleId::from_bytes([5; 32]),
            ModuleKind::PathBoundary,
            ModuleRoot::Directory(root_path.clone()),
            ModuleTreeBoundaryEvidence::new(ModuleKind::PathBoundary, 1, Some(member), None)?,
            0,
            1,
            1,
            0,
            false,
            0,
            false,
            0,
            false,
            ModuleTreeChildState::Leaf,
        )?;
        assert_eq!(entry.root(), &ModuleRoot::Directory(root_path));
        assert_eq!(entry.display_name().as_str().chars().count(), 256);
        assert!(entry.display_name().is_truncated());
        assert!(!entry.display_name().as_str().chars().any(char::is_control));
        Ok(())
    }

    #[test]
    fn page_rejects_unstable_order_parent_loops_and_mismatched_cursor() -> Result<(), Box<dyn Error>>
    {
        let make_entry = |id: u8| -> Result<ModuleTreeEntry, Box<dyn Error>> {
            Ok(ModuleTreeEntry::new(
                ModuleId::from_bytes([id; 32]),
                ModuleKind::PathBoundary,
                ModuleRoot::Directory(RepositoryPath::try_from_bytes(vec![id])?),
                ModuleTreeBoundaryEvidence::new(ModuleKind::PathBoundary, 0, None, None)?,
                0,
                0,
                0,
                0,
                false,
                0,
                false,
                0,
                false,
                ModuleTreeChildState::Leaf,
            )?)
        };
        let entries = vec![make_entry(2)?, make_entry(1)?];
        assert!(
            ModuleTreePage::new(
                IndexRunId::from_bytes([3; 32]),
                SnapshotId::from_bytes([4; 32]),
                None,
                2,
                0,
                entries,
                None,
            )
            .is_err()
        );
        assert!(
            ModuleTreePage::new(
                IndexRunId::from_bytes([3; 32]),
                SnapshotId::from_bytes([4; 32]),
                None,
                1,
                0,
                vec![make_entry(1)?],
                Some(ModuleId::from_bytes([9; 32])),
            )
            .is_err()
        );
        assert!(
            ModuleTreePage::new(
                IndexRunId::from_bytes([3; 32]),
                SnapshotId::from_bytes([4; 32]),
                Some(ModuleId::from_bytes([1; 32])),
                1,
                0,
                vec![make_entry(1)?],
                None,
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn use_case_reports_bounded_progress_and_honors_cancellation() -> Result<(), Box<dyn Error>> {
        let project = project()?;
        let query = ModuleTreeQuery::new(None, None, ModuleTreePageSize::DEFAULT);
        let control = RecordingControl::default();
        let result = block_on(
            GetModuleTreePage::new(Arc::new(RecordingStore)).execute(&project, &query, &control),
        )?;
        assert!(matches!(result, ModuleTreeLoadResult::Page(_)));
        let progress = control
            .progress
            .lock()
            .map_err(|_| std::io::Error::other("progress lock poisoned"))?;
        assert_eq!(progress.len(), 2);
        assert_eq!(progress[0].completed(), Some(0));
        assert_eq!(progress[1].completed(), Some(2));
        drop(progress);

        let cancelled = RecordingControl {
            cancelled: true,
            ..RecordingControl::default()
        };
        assert_eq!(
            block_on(
                GetModuleTreePage::new(Arc::new(RecordingStore))
                    .execute(&project, &query, &cancelled)
            ),
            Err(ModuleTreeFailure::Cancelled)
        );
        Ok(())
    }

    fn revision(path: &str, byte: u8) -> Result<FileRevision, Box<dyn Error>> {
        Ok(FileRevision::new(
            RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
            ContentHash::from_bytes([byte; 32]),
        ))
    }

    fn project() -> Result<ProjectIdentity, Box<dyn Error>> {
        let root = std::env::current_dir()?.canonicalize()?;
        let repository_id = RepositoryId::from_bytes([1; 32]);
        let repository = RepositoryIdentity::new(
            repository_id,
            CanonicalDirectory::from_canonicalized(root.clone())?,
            None,
        );
        let worktree = WorktreeIdentity::new(
            WorktreeId::from_bytes([3; 32]),
            WorktreeAnchorId::from_bytes([4; 32]),
            repository_id,
            CanonicalDirectory::from_canonicalized(root)?,
        );
        Ok(ProjectIdentity::new(
            repository,
            worktree,
            GitHead::Unborn {
                reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
            },
        )?)
    }
}
