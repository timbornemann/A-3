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
    Ok(())
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
}
