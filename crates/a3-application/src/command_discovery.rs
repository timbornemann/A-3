use a3_domain::{
    AgentRunId, AgentRunTimestamp, CommandCatalogError, CommandDiscoveryEvidence,
    CommandDiscoverySchemaVersion, DiscoveredCommand, DiscoveredCommandError, DiscoveredCommandId,
    DiscoveredCommandKind, DiscoveredCommandProcessError, FileRevision, ProcessSpec,
    ProjectCommandAllowlist, ProjectCommandAllowlistError, ProjectCommandCatalog, ProjectIdentity,
    PublishedIndex, RepositoryPath, RepositoryPathError, SyntaxProvider, TaskStepId,
    UnresolvedGraphTarget, WorkspaceDirectory, WorktreeId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

const MAX_ALLOWLIST_STORE_VERSION: u64 = i64::MAX as u64;

/// Deterministically derives safe direct-argv templates from one published index.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiscoverProjectCommands;

impl DiscoverProjectCommands {
    /// Uses only current manifest revisions and manifest-provider relationships as authority.
    pub fn execute(
        self,
        worktree_id: WorktreeId,
        index: &PublishedIndex,
    ) -> Result<ProjectCommandCatalog, CommandDiscoveryFailure> {
        let mut commands = Vec::new();
        discover_rust(index, &mut commands)?;
        discover_node(index, &mut commands)?;
        discover_python(index, &mut commands)?;
        ProjectCommandCatalog::new(CommandDiscoverySchemaVersion::V1, worktree_id, commands)
            .map_err(CommandDiscoveryFailure::Catalog)
    }
}

/// Deterministic manifest projection failed a bounded domain invariant.
#[derive(Debug)]
pub enum CommandDiscoveryFailure {
    /// One supported manifest produced an invalid bounded command.
    Command(DiscoveredCommandError),
    /// The complete catalog was invalid or exceeded its fixed bound.
    Catalog(CommandCatalogError),
    /// A manifest parent could not be represented as a normalized repository directory.
    ManifestPath(RepositoryPathError),
}

impl fmt::Display for CommandDiscoveryFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Command(_) => "manifest command is invalid",
            Self::Catalog(_) => "project command catalog is invalid",
            Self::ManifestPath(_) => "manifest package directory is invalid",
        })
    }
}

impl Error for CommandDiscoveryFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Command(source) => Some(source),
            Self::Catalog(source) => Some(source),
            Self::ManifestPath(source) => Some(source),
        }
    }
}

impl From<DiscoveredCommandError> for CommandDiscoveryFailure {
    fn from(value: DiscoveredCommandError) -> Self {
        Self::Command(value)
    }
}

/// Monotone compare-and-swap version of the durable project confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CommandAllowlistStoreVersion(u64);

impl CommandAllowlistStoreVersion {
    /// Creates a positive version representable by local SQL storage.
    pub const fn new(value: u64) -> Result<Self, CommandAllowlistStoreVersionError> {
        if value == 0 || value > MAX_ALLOWLIST_STORE_VERSION {
            return Err(CommandAllowlistStoreVersionError { value });
        }
        Ok(Self(value))
    }

    /// Returns the durable integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Invalid local allowlist version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandAllowlistStoreVersionError {
    value: u64,
}

impl fmt::Display for CommandAllowlistStoreVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "command allowlist store version {} is invalid",
            self.value
        )
    }
}

impl Error for CommandAllowlistStoreVersionError {}

/// One current durable explicit confirmation and its CAS version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredProjectCommandAllowlist {
    version: CommandAllowlistStoreVersion,
    allowlist: ProjectCommandAllowlist,
}

impl StoredProjectCommandAllowlist {
    /// Binds a reconstructed confirmation to its positive store version.
    #[must_use]
    pub const fn new(
        version: CommandAllowlistStoreVersion,
        allowlist: ProjectCommandAllowlist,
    ) -> Self {
        Self { version, allowlist }
    }

    /// Returns the optimistic concurrency version.
    #[must_use]
    pub const fn version(&self) -> CommandAllowlistStoreVersion {
        self.version
    }

    /// Returns the exact current user confirmation.
    #[must_use]
    pub const fn allowlist(&self) -> &ProjectCommandAllowlist {
        &self.allowlist
    }
}

/// Future returned by the object-safe project command-confirmation boundary.
pub type CommandAllowlistStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, CommandAllowlistStoreFailure>> + Send + 'a>>;

/// Durable private-worktree boundary for explicit command confirmations.
pub trait CommandAllowlistStore: fmt::Debug + Send + Sync {
    /// Loads the latest append-only confirmation, if one exists.
    fn load_current<'a>(
        &'a self,
        project: &'a ProjectIdentity,
    ) -> CommandAllowlistStoreFuture<'a, Option<StoredProjectCommandAllowlist>>;

    /// Appends a new confirmation only if the expected latest version still matches.
    fn append<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected: Option<CommandAllowlistStoreVersion>,
        confirmation: &'a ProjectCommandAllowlist,
    ) -> CommandAllowlistStoreFuture<'a, StoredProjectCommandAllowlist>;
}

/// Stable failure classification for local project command confirmation storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAllowlistStoreFailure {
    /// Local storage could not be reached or written.
    Unavailable,
    /// Local database integrity checks failed.
    Corrupt,
    /// Schema is newer than this application build.
    UnsupportedSchema,
    /// Durable fields violated domain invariants.
    InvalidStoredData,
    /// The owning project identity did not match the database.
    ProjectMismatch,
    /// Another writer appended a confirmation first.
    VersionConflict,
}

impl fmt::Display for CommandAllowlistStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "command allowlist storage is unavailable",
            Self::Corrupt => "command allowlist storage is corrupt",
            Self::UnsupportedSchema => "command allowlist storage uses an unsupported schema",
            Self::InvalidStoredData => "command allowlist storage contains invalid data",
            Self::ProjectMismatch => "command allowlist storage belongs to another project",
            Self::VersionConflict => "command allowlist changed concurrently",
        })
    }
}

impl Error for CommandAllowlistStoreFailure {}

/// Loads the current explicit project command confirmation.
#[derive(Debug, Clone, Copy)]
pub struct LoadProjectCommandAllowlist<'a> {
    store: &'a dyn CommandAllowlistStore,
}

impl<'a> LoadProjectCommandAllowlist<'a> {
    /// Creates the use case from its narrow local persistence capability.
    #[must_use]
    pub const fn new(store: &'a dyn CommandAllowlistStore) -> Self {
        Self { store }
    }

    /// Loads only the latest append-only confirmation.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
    ) -> Result<Option<StoredProjectCommandAllowlist>, CommandAllowlistStoreFailure> {
        self.store.load_current(project).await
    }
}

/// Confirms a displayed catalog subset and appends it using optimistic concurrency.
#[derive(Debug, Clone, Copy)]
pub struct ConfirmProjectCommandAllowlist<'a> {
    store: &'a dyn CommandAllowlistStore,
}

impl<'a> ConfirmProjectCommandAllowlist<'a> {
    /// Creates the use case from its narrow local persistence capability.
    #[must_use]
    pub const fn new(store: &'a dyn CommandAllowlistStore) -> Self {
        Self { store }
    }

    /// Persists only IDs from the exact catalog the user inspected.
    pub async fn execute(
        self,
        project: &ProjectIdentity,
        catalog: &ProjectCommandCatalog,
        command_ids: Vec<DiscoveredCommandId>,
        confirmed_at: AgentRunTimestamp,
        expected: Option<CommandAllowlistStoreVersion>,
    ) -> Result<StoredProjectCommandAllowlist, ConfirmProjectCommandAllowlistError> {
        if project.worktree().id() != catalog.worktree_id() {
            return Err(ConfirmProjectCommandAllowlistError::ProjectMismatch);
        }
        let allowlist = ProjectCommandAllowlist::confirm(catalog, command_ids, confirmed_at)
            .map_err(ConfirmProjectCommandAllowlistError::InvalidConfirmation)?;
        self.store
            .append(project, expected, &allowlist)
            .await
            .map_err(ConfirmProjectCommandAllowlistError::Store)
    }
}

/// Explicit project command confirmation failed before changing durable state.
#[derive(Debug)]
pub enum ConfirmProjectCommandAllowlistError {
    /// The catalog belonged to another worktree.
    ProjectMismatch,
    /// The selected command set did not belong to the displayed catalog.
    InvalidConfirmation(ProjectCommandAllowlistError),
    /// Durable storage rejected the append.
    Store(CommandAllowlistStoreFailure),
}

impl fmt::Display for ConfirmProjectCommandAllowlistError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ProjectMismatch => "command catalog belongs to another project",
            Self::InvalidConfirmation(_) => "project command selection is invalid",
            Self::Store(_) => "project command confirmation could not be stored",
        })
    }
}

impl Error for ConfirmProjectCommandAllowlistError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidConfirmation(source) => Some(source),
            Self::Store(source) => Some(source),
            Self::ProjectMismatch => None,
        }
    }
}

/// Forms an executable safe `ProcessSpec` from current evidence, confirmation, and task step.
#[derive(Debug, Clone, Copy, Default)]
pub struct PrepareDiscoveredCommand;

impl PrepareDiscoveredCommand {
    /// Rejects a stale catalog, unconfirmed command, or cross-worktree confirmation.
    pub fn execute(
        self,
        catalog: &ProjectCommandCatalog,
        confirmation: &StoredProjectCommandAllowlist,
        run_id: AgentRunId,
        step_id: TaskStepId,
        command_id: DiscoveredCommandId,
    ) -> Result<ProcessSpec, DiscoveredCommandProcessError> {
        catalog.bind_confirmed(confirmation.allowlist(), run_id, step_id, command_id)
    }
}

fn discover_rust(
    index: &PublishedIndex,
    commands: &mut Vec<DiscoveredCommand>,
) -> Result<(), CommandDiscoveryFailure> {
    for manifest in index.publication().manifest_files() {
        if file_name(manifest.path()) != b"Cargo.toml" {
            continue;
        }
        let directory = workspace_directory(manifest.path())?;
        let evidence = || vec![CommandDiscoveryEvidence::File(manifest.clone())];
        commands.push(DiscoveredCommand::try_new(
            DiscoveredCommandKind::Test,
            directory.clone(),
            "cargo".to_owned(),
            strings(&["test", "--offline", "--locked"]),
            evidence(),
        )?);
        commands.push(DiscoveredCommand::try_new(
            DiscoveredCommandKind::Build,
            directory.clone(),
            "cargo".to_owned(),
            strings(&["build", "--offline", "--locked"]),
            evidence(),
        )?);
        commands.push(DiscoveredCommand::try_new(
            DiscoveredCommandKind::Lint,
            directory.clone(),
            "cargo".to_owned(),
            strings(&[
                "clippy",
                "--offline",
                "--locked",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ]),
            evidence(),
        )?);
        commands.push(DiscoveredCommand::try_new(
            DiscoveredCommandKind::Format,
            directory,
            "cargo".to_owned(),
            strings(&["fmt", "--", "--check"]),
            evidence(),
        )?);
    }
    Ok(())
}

fn discover_node(
    index: &PublishedIndex,
    commands: &mut Vec<DiscoveredCommand>,
) -> Result<(), CommandDiscoveryFailure> {
    let manifests = index.publication().manifest_files();
    for candidate in index.publication().graph().unresolved() {
        if candidate.provider() != SyntaxProvider::Manifest
            || file_name(candidate.evidence().revision().path()) != b"package.json"
        {
            continue;
        }
        let UnresolvedGraphTarget::Reference(reference) = candidate.target() else {
            continue;
        };
        let Some(script) = reference.as_str().strip_prefix("script:") else {
            continue;
        };
        let Some(kind) = node_script_kind(script) else {
            continue;
        };
        let package_root = parent_path(candidate.evidence().revision().path());
        let Some((manager, manager_evidence)) = node_package_manager(package_root, manifests)
        else {
            continue;
        };
        commands.push(DiscoveredCommand::try_new(
            kind,
            workspace_directory(candidate.evidence().revision().path())?,
            manager.to_owned(),
            vec!["run".to_owned(), script.to_owned()],
            vec![
                CommandDiscoveryEvidence::Source(candidate.evidence().clone()),
                CommandDiscoveryEvidence::File(manager_evidence.clone()),
            ],
        )?);
    }
    Ok(())
}

fn discover_python(
    index: &PublishedIndex,
    commands: &mut Vec<DiscoveredCommand>,
) -> Result<(), CommandDiscoveryFailure> {
    let mut roots = BTreeMap::<Vec<u8>, Vec<&FileRevision>>::new();
    for manifest in index.publication().manifest_files() {
        if matches!(
            file_name(manifest.path()),
            b"pyproject.toml" | b"setup.cfg" | b"setup.py"
        ) {
            roots
                .entry(parent_path(manifest.path()).to_vec())
                .or_default()
                .push(manifest);
        }
    }
    let has_python_manifest = !roots.is_empty();
    for (root, manifests) in roots {
        let mut references = BTreeMap::<String, CommandDiscoveryEvidence>::new();
        let mut has_build = None;
        for candidate in index.publication().graph().unresolved() {
            if candidate.provider() != SyntaxProvider::Manifest
                || parent_path(candidate.evidence().revision().path()) != root
                || !manifests
                    .iter()
                    .any(|manifest| manifest.path() == candidate.evidence().revision().path())
            {
                continue;
            }
            let UnresolvedGraphTarget::Reference(reference) = candidate.target() else {
                continue;
            };
            let evidence = CommandDiscoveryEvidence::Source(candidate.evidence().clone());
            references
                .entry(reference.as_str().to_ascii_lowercase())
                .or_insert_with(|| evidence.clone());
            if candidate.kind() == a3_domain::SyntaxRelationKind::Builds {
                has_build.get_or_insert(evidence);
            }
        }
        let directory = directory_from_parent_bytes(&root)?;
        if let Some(evidence) = first_reference(&references, |reference| {
            reference == "pytest" || reference.starts_with("pytest:")
        }) {
            commands.push(python_command(
                DiscoveredCommandKind::Test,
                directory.clone(),
                &["-m", "pytest"],
                evidence,
            )?);
        }
        if let Some(evidence) = has_build {
            commands.push(python_command(
                DiscoveredCommandKind::Build,
                directory.clone(),
                &["-m", "build", "--no-isolation"],
                evidence,
            )?);
        }
        if let Some(evidence) = first_reference(&references, |reference| reference == "ruff") {
            commands.push(python_command(
                DiscoveredCommandKind::Lint,
                directory.clone(),
                &["-m", "ruff", "check", "."],
                evidence.clone(),
            )?);
            commands.push(python_command(
                DiscoveredCommandKind::Format,
                directory.clone(),
                &["-m", "ruff", "format", "--check", "."],
                evidence,
            )?);
        }
        if let Some(evidence) = first_reference(&references, |reference| reference == "black") {
            commands.push(python_command(
                DiscoveredCommandKind::Format,
                directory.clone(),
                &["-m", "black", "--check", "."],
                evidence,
            )?);
        }
        if let Some(evidence) = first_reference(&references, |reference| reference == "mypy") {
            commands.push(python_command(
                DiscoveredCommandKind::Lint,
                directory,
                &["-m", "mypy", "."],
                evidence,
            )?);
        }
    }
    if !has_python_manifest {
        discover_python_unittest(index, commands)?;
    }
    Ok(())
}

/// Closed standard-library fallback for a manifest-free Python worktree. Every test revision is
/// bound into the command identity; an oversized set is left unresolved rather than truncated.
fn discover_python_unittest(
    index: &PublishedIndex,
    commands: &mut Vec<DiscoveredCommand>,
) -> Result<(), CommandDiscoveryFailure> {
    let mut tests = index
        .publication()
        .graph()
        .files()
        .iter()
        .filter(|revision| is_python_test_path(revision.path()))
        .collect::<Vec<_>>();
    tests.sort_by(|left, right| left.path().cmp(right.path()));
    let has_python_source = index.publication().graph().files().iter().any(|revision| {
        file_name(revision.path()).ends_with(b".py") && !is_python_test_path(revision.path())
    });
    if !has_python_source || tests.is_empty() || tests.len() > 16 {
        return Ok(());
    }
    let roots = tests
        .iter()
        .map(|revision| parent_path(revision.path()).to_vec())
        .collect::<BTreeSet<_>>();
    let arguments = if roots.len() == 1 {
        let Some(root) = roots.first() else {
            return Ok(());
        };
        if root.is_empty() {
            strings(&["-B", "-m", "unittest", "discover"])
        } else {
            let Ok(root) = std::str::from_utf8(root) else {
                return Ok(());
            };
            let mut arguments = strings(&["-B", "-m", "unittest", "discover", "-s"]);
            arguments.push(root.to_owned());
            arguments
        }
    } else {
        // Discovery has one start directory, so mixed roots would otherwise make a valid
        // manifest-free project unverifiable. Bind every exact current test path into one argv
        // instead; unittest accepts repository-relative file paths without a shell.
        let mut arguments = strings(&["-B", "-m", "unittest"]);
        for test in &tests {
            let Ok(path) = std::str::from_utf8(test.path().as_bytes()) else {
                return Ok(());
            };
            arguments.push(path.to_owned());
        }
        arguments
    };
    commands.push(DiscoveredCommand::try_new(
        DiscoveredCommandKind::Test,
        WorkspaceDirectory::Root,
        "python".to_owned(),
        arguments,
        tests
            .into_iter()
            .cloned()
            .map(CommandDiscoveryEvidence::File)
            .collect(),
    )?);
    Ok(())
}

fn is_python_test_path(path: &RepositoryPath) -> bool {
    let name = file_name(path);
    name.starts_with(b"test") && name.ends_with(b".py")
}

fn python_command(
    kind: DiscoveredCommandKind,
    directory: WorkspaceDirectory,
    arguments: &[&str],
    evidence: CommandDiscoveryEvidence,
) -> Result<DiscoveredCommand, DiscoveredCommandError> {
    DiscoveredCommand::try_new(
        kind,
        directory,
        "python".to_owned(),
        strings(arguments),
        vec![evidence],
    )
}

fn first_reference(
    references: &BTreeMap<String, CommandDiscoveryEvidence>,
    predicate: impl Fn(&str) -> bool,
) -> Option<CommandDiscoveryEvidence> {
    references
        .iter()
        .find(|(reference, _)| predicate(reference))
        .map(|(_, evidence)| evidence.clone())
}

fn node_script_kind(script: &str) -> Option<DiscoveredCommandKind> {
    if script == "test" || script.starts_with("test:") {
        Some(DiscoveredCommandKind::Test)
    } else if script == "build" || script.starts_with("build:") {
        Some(DiscoveredCommandKind::Build)
    } else if script == "lint" || script.starts_with("lint:") {
        Some(DiscoveredCommandKind::Lint)
    } else if script == "format" || script.starts_with("format:") {
        Some(DiscoveredCommandKind::Format)
    } else {
        None
    }
}

fn node_package_manager<'a>(
    package_root: &[u8],
    manifests: &'a [FileRevision],
) -> Option<(&'static str, &'a FileRevision)> {
    let mut candidates = Vec::new();
    for manifest in manifests {
        let manager = match file_name(manifest.path()) {
            b"pnpm-lock.yaml" | b"pnpm-workspace.yaml" => "pnpm",
            b"package-lock.json" | b"npm-shrinkwrap.json" => "npm",
            b"yarn.lock" => "yarn",
            _ => continue,
        };
        let marker_root = parent_path(manifest.path());
        if is_path_ancestor(marker_root, package_root) {
            candidates.push((marker_root.len(), manager, manifest));
        }
    }
    let maximum_depth = candidates.iter().map(|candidate| candidate.0).max()?;
    let nearest = candidates
        .into_iter()
        .filter(|candidate| candidate.0 == maximum_depth)
        .collect::<Vec<_>>();
    let managers = nearest
        .iter()
        .map(|candidate| candidate.1)
        .collect::<BTreeSet<_>>();
    if managers.len() != 1 {
        return None;
    }
    nearest
        .into_iter()
        .min_by(|left, right| left.2.path().cmp(right.2.path()))
        .map(|(_, manager, manifest)| (manager, manifest))
}

fn workspace_directory(
    manifest: &RepositoryPath,
) -> Result<WorkspaceDirectory, CommandDiscoveryFailure> {
    directory_from_parent_bytes(parent_path(manifest))
}

fn directory_from_parent_bytes(
    parent: &[u8],
) -> Result<WorkspaceDirectory, CommandDiscoveryFailure> {
    if parent.is_empty() {
        Ok(WorkspaceDirectory::Root)
    } else {
        RepositoryPath::try_from_bytes(parent.to_vec())
            .map(WorkspaceDirectory::Subtree)
            .map_err(CommandDiscoveryFailure::ManifestPath)
    }
}

fn parent_path(path: &RepositoryPath) -> &[u8] {
    path.as_bytes()
        .iter()
        .rposition(|byte| *byte == b'/')
        .and_then(|position| path.as_bytes().get(..position))
        .unwrap_or_default()
}

fn file_name(path: &RepositoryPath) -> &[u8] {
    path.as_bytes()
        .rsplit(|byte| *byte == b'/')
        .next()
        .unwrap_or_default()
}

fn is_path_ancestor(ancestor: &[u8], descendant: &[u8]) -> bool {
    ancestor.is_empty()
        || descendant == ancestor
        || descendant
            .strip_prefix(ancestor)
            .is_some_and(|suffix| suffix.first() == Some(&b'/'))
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_kind_has_no_install_variant_and_rejects_unrelated_scripts() {
        assert_eq!(
            node_script_kind("test:unit"),
            Some(DiscoveredCommandKind::Test)
        );
        assert_eq!(node_script_kind("install"), None);
        assert_eq!(node_script_kind("postinstall"), None);
        assert_eq!(node_script_kind("prepare"), None);
    }

    #[test]
    fn package_manager_requires_one_nearest_indexed_marker() {
        let package = b"packages/web";
        assert!(is_path_ancestor(b"", package));
        assert!(is_path_ancestor(b"packages", package));
        assert!(!is_path_ancestor(b"package", package));
        assert!(!is_path_ancestor(b"packages/api", package));
    }

    #[test]
    fn allowlist_versions_are_positive_and_bounded() -> Result<(), Box<dyn Error>> {
        assert!(CommandAllowlistStoreVersion::new(0).is_err());
        assert_eq!(CommandAllowlistStoreVersion::new(1)?.get(), 1);
        assert!(CommandAllowlistStoreVersion::new((i64::MAX as u64) + 1).is_err());
        Ok(())
    }

    #[test]
    fn manifest_free_python_tests_discover_only_the_closed_unittest_command()
    -> Result<(), Box<dyn Error>> {
        let test = a3_domain::FileRevision::new(
            RepositoryPath::try_from_bytes(b"tests/test_server.py".to_vec())?,
            a3_domain::ContentHash::from_bytes([1; 32]),
        );
        let source = a3_domain::FileRevision::new(
            RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
            a3_domain::ContentHash::from_bytes([2; 32]),
        );
        let index = published_index(vec![source, test.clone()])?;
        let catalog =
            DiscoverProjectCommands.execute(a3_domain::WorktreeId::from_bytes([3; 32]), &index)?;
        assert_eq!(catalog.commands().len(), 1);
        let command = &catalog.commands()[0];
        assert_eq!(command.kind(), DiscoveredCommandKind::Test);
        assert_eq!(command.executable().as_str(), "python");
        assert_eq!(
            command
                .arguments()
                .iter()
                .map(|argument| argument.as_str())
                .collect::<Vec<_>>(),
            ["-B", "-m", "unittest", "discover", "-s", "tests"]
        );
        assert_eq!(command.evidence().len(), 1);
        assert_eq!(command.evidence()[0].revision(), &test);
        Ok(())
    }

    #[test]
    fn manifest_free_unittest_discovery_matches_the_exact_default_filename_pattern()
    -> Result<(), Box<dyn Error>> {
        let source = a3_domain::FileRevision::new(
            RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
            a3_domain::ContentHash::from_bytes([6; 32]),
        );
        let unsupported_name = a3_domain::FileRevision::new(
            RepositoryPath::try_from_bytes(b"tests/server_test.py".to_vec())?,
            a3_domain::ContentHash::from_bytes([7; 32]),
        );
        let index = published_index(vec![source, unsupported_name])?;
        let catalog =
            DiscoverProjectCommands.execute(a3_domain::WorktreeId::from_bytes([8; 32]), &index)?;
        assert!(catalog.commands().is_empty());
        Ok(())
    }

    #[test]
    fn manifest_free_unittest_supports_multiple_test_roots_and_requires_python_source()
    -> Result<(), Box<dyn Error>> {
        let test_a = a3_domain::FileRevision::new(
            RepositoryPath::try_from_bytes(b"tests/test_a.py".to_vec())?,
            a3_domain::ContentHash::from_bytes([9; 32]),
        );
        let test_b = a3_domain::FileRevision::new(
            RepositoryPath::try_from_bytes(b"spec/test_b.py".to_vec())?,
            a3_domain::ContentHash::from_bytes([10; 32]),
        );
        let source = a3_domain::FileRevision::new(
            RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
            a3_domain::ContentHash::from_bytes([11; 32]),
        );
        let worktree = a3_domain::WorktreeId::from_bytes([12; 32]);
        let without_source = published_index(vec![test_a.clone()])?;
        assert!(
            DiscoverProjectCommands
                .execute(worktree, &without_source)?
                .commands()
                .is_empty()
        );
        let multiple_roots = published_index(vec![source, test_a.clone(), test_b.clone()])?;
        let catalog = DiscoverProjectCommands.execute(worktree, &multiple_roots)?;
        assert_eq!(catalog.commands().len(), 1);
        assert_eq!(
            catalog.commands()[0]
                .arguments()
                .iter()
                .map(|argument| argument.as_str())
                .collect::<Vec<_>>(),
            ["-B", "-m", "unittest", "spec/test_b.py", "tests/test_a.py"]
        );
        assert_eq!(catalog.commands()[0].evidence().len(), 2);
        assert_eq!(catalog.commands()[0].evidence()[0].revision(), &test_b);
        assert_eq!(catalog.commands()[0].evidence()[1].revision(), &test_a);
        Ok(())
    }

    fn published_index(
        files: Vec<a3_domain::FileRevision>,
    ) -> Result<a3_domain::PublishedIndex, Box<dyn Error>> {
        let snapshot_id = a3_domain::SnapshotId::from_bytes([4; 32]);
        let graph = a3_domain::LinkedGraph::new(
            snapshot_id,
            files.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )?;
        let ranking = a3_domain::RankProjection::new(
            snapshot_id,
            a3_domain::RankingPolicyVersion::v1(),
            Vec::new(),
        )?;
        let policy = a3_domain::ModulePolicyVersion::v1();
        let card = a3_domain::RepositoryCard::new(
            snapshot_id,
            policy,
            Vec::new(),
            Vec::new(),
            a3_domain::ModuleSymbolSet::empty(),
            u32::try_from(files.len())?,
            0,
        )?;
        let modules =
            a3_domain::ModuleProjection::new(snapshot_id, policy, Vec::new(), Vec::new(), card)?;
        let publication = a3_domain::IndexPublication::new(graph, ranking, files, modules)?;
        let run = a3_domain::IndexRunRecord::new(
            a3_domain::IndexRunId::from_bytes([5; 32]),
            snapshot_id,
            a3_domain::RankingPolicyVersion::v1(),
            a3_domain::IndexRunSequence::new(1)?,
            a3_domain::IndexRunStatus::Published,
        );
        Ok(a3_domain::PublishedIndex::new(run, publication)?)
    }
}
