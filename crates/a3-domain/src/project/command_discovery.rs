use super::{
    AgentRunId, AgentRunTimestamp, CommandCatalogId, DiscoveredCommandId, EvidenceRef,
    FileRevision, ProcessArgument, ProcessArgumentError, ProcessEnvironmentVariable,
    ProcessEnvironmentVariableError, ProcessExecutable, ProcessExecutableError,
    ProcessExecutionMode, ProcessNetworkScope, ProcessOutputLimit, ProcessOutputLimitError,
    ProcessPlanBinding, ProcessSpec, ProcessSpecError, ProcessSpecSchemaVersion, ProcessTimeout,
    ProcessTimeoutError, TaskStepId, WorkspaceDirectory, WorktreeId,
};
use std::cmp::Ordering;
use std::error::Error;
use std::fmt;

const MAX_DISCOVERED_COMMANDS: usize = 256;
const MAX_COMMAND_EVIDENCE: usize = 16;
const RETAINED_OUTPUT_BYTES: u32 = 1024 * 1024;

/// Version of deterministic manifest-to-command discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CommandDiscoverySchemaVersion {
    /// Initial Rust, Node, and Python discovery contract.
    V1,
}

/// Closed safe-command category; package installation is intentionally not representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiscoveredCommandKind {
    /// Executes project tests.
    Test,
    /// Builds project artifacts without installing dependencies.
    Build,
    /// Performs static diagnostics.
    Lint,
    /// Checks formatting without mutating files.
    Format,
}

impl DiscoveredCommandKind {
    /// Returns the stable persistence and display code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Build => "build",
            Self::Lint => "lint",
            Self::Format => "format",
        }
    }

    const fn timeout_millis(self) -> u64 {
        match self {
            Self::Test | Self::Build => 10 * 60 * 1_000,
            Self::Lint | Self::Format => 5 * 60 * 1_000,
        }
    }
}

/// Exact indexed source evidence supporting one discovered command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandEvidence {
    /// A complete manifest or package-manager marker revision.
    File(FileRevision),
    /// A precise manifest field or relationship range.
    Source(EvidenceRef),
}

impl CommandEvidence {
    /// Returns the immutable source-file revision.
    #[must_use]
    pub const fn revision(&self) -> &FileRevision {
        match self {
            Self::File(revision) => revision,
            Self::Source(evidence) => evidence.revision(),
        }
    }
}

/// One bounded, shell-free command derived exclusively from current index evidence.
#[derive(Clone, PartialEq, Eq)]
pub struct DiscoveredCommand {
    id: DiscoveredCommandId,
    kind: DiscoveredCommandKind,
    working_directory: WorkspaceDirectory,
    executable: ProcessExecutable,
    arguments: Vec<ProcessArgument>,
    environment_allowlist: Vec<ProcessEnvironmentVariable>,
    timeout: ProcessTimeout,
    stdout_limit: ProcessOutputLimit,
    stderr_limit: ProcessOutputLimit,
    evidence: Vec<CommandEvidence>,
}

impl DiscoveredCommand {
    /// Validates one direct argv template and binds it to bounded current evidence.
    pub fn try_new(
        kind: DiscoveredCommandKind,
        working_directory: WorkspaceDirectory,
        executable: String,
        arguments: Vec<String>,
        mut evidence: Vec<CommandEvidence>,
    ) -> Result<Self, DiscoveredCommandError> {
        if evidence.is_empty() || evidence.len() > MAX_COMMAND_EVIDENCE {
            return Err(DiscoveredCommandError::InvalidEvidenceCount {
                actual: evidence.len(),
            });
        }
        evidence.sort_by(compare_evidence);
        if evidence.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(DiscoveredCommandError::DuplicateEvidence);
        }
        let executable = ProcessExecutable::try_from_string(executable)
            .map_err(DiscoveredCommandError::Executable)?;
        let arguments = arguments
            .into_iter()
            .map(ProcessArgument::try_from_string)
            .collect::<Result<Vec<_>, _>>()
            .map_err(DiscoveredCommandError::Argument)?;
        let environment_allowlist = vec![
            ProcessEnvironmentVariable::try_from_string("PATH".to_owned())
                .map_err(DiscoveredCommandError::Environment)?,
        ];
        let timeout = ProcessTimeout::from_millis(kind.timeout_millis())
            .map_err(DiscoveredCommandError::Timeout)?;
        let stdout_limit = ProcessOutputLimit::new(RETAINED_OUTPUT_BYTES)
            .map_err(DiscoveredCommandError::OutputLimit)?;
        let stderr_limit = ProcessOutputLimit::new(RETAINED_OUTPUT_BYTES)
            .map_err(DiscoveredCommandError::OutputLimit)?;
        let id = derive_command_id(kind, &working_directory, &executable, &arguments, &evidence);
        Ok(Self {
            id,
            kind,
            working_directory,
            executable,
            arguments,
            environment_allowlist,
            timeout,
            stdout_limit,
            stderr_limit,
            evidence,
        })
    }

    /// Returns the exact evidence- and argv-bound command identity.
    #[must_use]
    pub const fn id(&self) -> DiscoveredCommandId {
        self.id
    }

    /// Returns the closed safe-command category.
    #[must_use]
    pub const fn kind(&self) -> DiscoveredCommandKind {
        self.kind
    }

    /// Returns the package-local worktree directory.
    #[must_use]
    pub const fn working_directory(&self) -> &WorkspaceDirectory {
        &self.working_directory
    }

    /// Returns the direct executable token.
    #[must_use]
    pub const fn executable(&self) -> &ProcessExecutable {
        &self.executable
    }

    /// Returns the exact ordered argv tail.
    #[must_use]
    pub fn arguments(&self) -> &[ProcessArgument] {
        &self.arguments
    }

    /// Returns every indexed revision or range supporting this command.
    #[must_use]
    pub fn evidence(&self) -> &[CommandEvidence] {
        &self.evidence
    }

    fn process_spec(
        &self,
        run_id: AgentRunId,
        worktree_id: WorktreeId,
        plan_binding: ProcessPlanBinding,
    ) -> Result<ProcessSpec, ProcessSpecError> {
        ProcessSpec::new(
            ProcessSpecSchemaVersion::V1,
            run_id,
            worktree_id,
            self.executable.clone(),
            self.arguments.clone(),
            self.working_directory.clone(),
            self.environment_allowlist.clone(),
            self.timeout,
            self.stdout_limit,
            self.stderr_limit,
            ProcessExecutionMode::KnownSafe,
            plan_binding,
            ProcessNetworkScope::Denied,
        )
    }
}

impl fmt::Debug for DiscoveredCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiscoveredCommand")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("working_directory", &self.working_directory)
            .field("executable", &self.executable)
            .field("argument_count", &self.arguments.len())
            .field("evidence_count", &self.evidence.len())
            .finish()
    }
}

/// A discovered command violated a fixed argv or evidence bound.
#[derive(Debug)]
pub enum DiscoveredCommandError {
    /// No evidence was supplied or the evidence set exceeded sixteen entries.
    InvalidEvidenceCount {
        /// Observed evidence count.
        actual: usize,
    },
    /// The evidence set repeated an identical locator.
    DuplicateEvidence,
    /// Executable token was invalid.
    Executable(ProcessExecutableError),
    /// One argv value was invalid.
    Argument(ProcessArgumentError),
    /// The fixed host-environment key was invalid.
    Environment(ProcessEnvironmentVariableError),
    /// The fixed category timeout was invalid.
    Timeout(ProcessTimeoutError),
    /// The fixed retained-output limit was invalid.
    OutputLimit(ProcessOutputLimitError),
}

impl fmt::Display for DiscoveredCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidEvidenceCount { .. } => "discovered command evidence count is invalid",
            Self::DuplicateEvidence => "discovered command repeats evidence",
            Self::Executable(_) => "discovered command executable is invalid",
            Self::Argument(_) => "discovered command argument is invalid",
            Self::Environment(_) => "discovered command environment is invalid",
            Self::Timeout(_) => "discovered command timeout is invalid",
            Self::OutputLimit(_) => "discovered command output limit is invalid",
        })
    }
}

impl Error for DiscoveredCommandError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Executable(source) => Some(source),
            Self::Argument(source) => Some(source),
            Self::Environment(source) => Some(source),
            Self::Timeout(source) => Some(source),
            Self::OutputLimit(source) => Some(source),
            Self::InvalidEvidenceCount { .. } | Self::DuplicateEvidence => None,
        }
    }
}

/// Complete deterministic command projection for one worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCommandCatalog {
    schema: CommandDiscoverySchemaVersion,
    worktree_id: WorktreeId,
    id: CommandCatalogId,
    commands: Vec<DiscoveredCommand>,
}

impl ProjectCommandCatalog {
    /// Canonicalizes the command set and derives its evidence-sensitive identity.
    pub fn new(
        schema: CommandDiscoverySchemaVersion,
        worktree_id: WorktreeId,
        mut commands: Vec<DiscoveredCommand>,
    ) -> Result<Self, CommandCatalogError> {
        if commands.len() > MAX_DISCOVERED_COMMANDS {
            return Err(CommandCatalogError::TooManyCommands {
                actual: commands.len(),
            });
        }
        commands.sort_by_key(DiscoveredCommand::id);
        if commands.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
            return Err(CommandCatalogError::DuplicateCommand);
        }
        let id = derive_catalog_id(schema, worktree_id, &commands);
        Ok(Self {
            schema,
            worktree_id,
            id,
            commands,
        })
    }

    /// Returns the discovery schema.
    #[must_use]
    pub const fn schema(&self) -> CommandDiscoverySchemaVersion {
        self.schema
    }

    /// Returns the owning worktree.
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    /// Returns the exact catalog identity.
    #[must_use]
    pub const fn id(&self) -> CommandCatalogId {
        self.id
    }

    /// Returns commands in stable identity order.
    #[must_use]
    pub fn commands(&self) -> &[DiscoveredCommand] {
        &self.commands
    }

    /// Creates a displayable, deliberately plan-unbound process specification.
    pub fn preview(
        &self,
        run_id: AgentRunId,
        command_id: DiscoveredCommandId,
    ) -> Result<ProcessSpec, DiscoveredCommandProcessError> {
        self.command(command_id)?
            .process_spec(run_id, self.worktree_id, ProcessPlanBinding::Unbound)
            .map_err(DiscoveredCommandProcessError::ProcessSpec)
    }

    /// Creates an automatically eligible spec only from an exact current allowlist and task step.
    pub fn bind_confirmed(
        &self,
        allowlist: &ProjectCommandAllowlist,
        run_id: AgentRunId,
        step_id: TaskStepId,
        command_id: DiscoveredCommandId,
    ) -> Result<ProcessSpec, DiscoveredCommandProcessError> {
        allowlist.authorize(self, command_id)?;
        self.command(command_id)?
            .process_spec(
                run_id,
                self.worktree_id,
                ProcessPlanBinding::Validated(step_id),
            )
            .map_err(DiscoveredCommandProcessError::ProcessSpec)
    }

    fn command(
        &self,
        command_id: DiscoveredCommandId,
    ) -> Result<&DiscoveredCommand, DiscoveredCommandProcessError> {
        self.commands
            .binary_search_by_key(&command_id, DiscoveredCommand::id)
            .ok()
            .and_then(|position| self.commands.get(position))
            .ok_or(DiscoveredCommandProcessError::UnknownCommand)
    }
}

/// Invalid complete command projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCatalogError {
    /// More than 256 safe commands were discovered.
    TooManyCommands {
        /// Observed command count.
        actual: usize,
    },
    /// Two identical evidence- and argv-bound commands were supplied.
    DuplicateCommand,
}

impl fmt::Display for CommandCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooManyCommands { .. } => "command catalog exceeds 256 commands",
            Self::DuplicateCommand => "command catalog contains a duplicate command",
        })
    }
}

impl Error for CommandCatalogError {}

/// Explicit user confirmation of a bounded subset of one exact project catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectCommandAllowlist {
    worktree_id: WorktreeId,
    catalog_id: CommandCatalogId,
    command_ids: Vec<DiscoveredCommandId>,
    confirmed_at: AgentRunTimestamp,
}

impl ProjectCommandAllowlist {
    /// Confirms only IDs present in the exact displayed catalog.
    pub fn confirm(
        catalog: &ProjectCommandCatalog,
        mut command_ids: Vec<DiscoveredCommandId>,
        confirmed_at: AgentRunTimestamp,
    ) -> Result<Self, ProjectCommandAllowlistError> {
        if command_ids.is_empty() || command_ids.len() > MAX_DISCOVERED_COMMANDS {
            return Err(ProjectCommandAllowlistError::InvalidCommandCount {
                actual: command_ids.len(),
            });
        }
        command_ids.sort();
        if command_ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ProjectCommandAllowlistError::DuplicateCommand);
        }
        if command_ids.iter().any(|id| {
            catalog
                .commands
                .binary_search_by_key(id, DiscoveredCommand::id)
                .is_err()
        }) {
            return Err(ProjectCommandAllowlistError::UnknownCommand);
        }
        Ok(Self {
            worktree_id: catalog.worktree_id,
            catalog_id: catalog.id,
            command_ids,
            confirmed_at,
        })
    }

    /// Reconstructs a persisted confirmation before matching it against a fresh catalog.
    pub fn from_stored(
        worktree_id: WorktreeId,
        catalog_id: CommandCatalogId,
        command_ids: Vec<DiscoveredCommandId>,
        confirmed_at: AgentRunTimestamp,
    ) -> Result<Self, ProjectCommandAllowlistError> {
        if command_ids.is_empty() || command_ids.len() > MAX_DISCOVERED_COMMANDS {
            return Err(ProjectCommandAllowlistError::InvalidCommandCount {
                actual: command_ids.len(),
            });
        }
        if command_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(ProjectCommandAllowlistError::NonCanonicalCommands);
        }
        Ok(Self {
            worktree_id,
            catalog_id,
            command_ids,
            confirmed_at,
        })
    }

    /// Returns the owning worktree.
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    /// Returns the exact catalog revision that the user inspected.
    #[must_use]
    pub const fn catalog_id(&self) -> CommandCatalogId {
        self.catalog_id
    }

    /// Returns the confirmed command IDs in canonical order.
    #[must_use]
    pub fn command_ids(&self) -> &[DiscoveredCommandId] {
        &self.command_ids
    }

    /// Returns the durable confirmation timestamp.
    #[must_use]
    pub const fn confirmed_at(&self) -> AgentRunTimestamp {
        self.confirmed_at
    }

    fn authorize(
        &self,
        catalog: &ProjectCommandCatalog,
        command_id: DiscoveredCommandId,
    ) -> Result<(), DiscoveredCommandProcessError> {
        if self.worktree_id != catalog.worktree_id {
            return Err(DiscoveredCommandProcessError::WorktreeMismatch);
        }
        if self.catalog_id != catalog.id {
            return Err(DiscoveredCommandProcessError::StaleAllowlist);
        }
        if self.command_ids.binary_search(&command_id).is_err() {
            return Err(DiscoveredCommandProcessError::CommandNotConfirmed);
        }
        Ok(())
    }
}

/// Invalid explicit project command confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectCommandAllowlistError {
    /// No commands or more than 256 commands were selected.
    InvalidCommandCount {
        /// Observed command count.
        actual: usize,
    },
    /// A command was selected more than once.
    DuplicateCommand,
    /// Selection referred to a command outside the displayed catalog.
    UnknownCommand,
    /// Persisted IDs were not in strict canonical order.
    NonCanonicalCommands,
}

impl fmt::Display for ProjectCommandAllowlistError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidCommandCount { .. } => "project command allowlist size is invalid",
            Self::DuplicateCommand => "project command allowlist repeats a command",
            Self::UnknownCommand => "project command allowlist contains an unknown command",
            Self::NonCanonicalCommands => "stored project command allowlist is not canonical",
        })
    }
}

impl Error for ProjectCommandAllowlistError {}

/// A preview or executable command could not be formed from the exact current catalog.
#[derive(Debug)]
pub enum DiscoveredCommandProcessError {
    /// The requested ID was absent from the current catalog.
    UnknownCommand,
    /// The confirmation belongs to another worktree.
    WorktreeMismatch,
    /// Manifest evidence changed after confirmation.
    StaleAllowlist,
    /// The exact command was not selected by the user.
    CommandNotConfirmed,
    /// The bounded process invariant unexpectedly rejected the template.
    ProcessSpec(ProcessSpecError),
}

impl fmt::Display for DiscoveredCommandProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnknownCommand => "discovered command is not in the current catalog",
            Self::WorktreeMismatch => "command allowlist belongs to another worktree",
            Self::StaleAllowlist => "command allowlist does not match current manifest evidence",
            Self::CommandNotConfirmed => "discovered command was not confirmed",
            Self::ProcessSpec(_) => "discovered command could not form a process specification",
        })
    }
}

impl Error for DiscoveredCommandProcessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ProcessSpec(source) => Some(source),
            _ => None,
        }
    }
}

fn compare_evidence(left: &CommandEvidence, right: &CommandEvidence) -> Ordering {
    evidence_kind(left)
        .cmp(&evidence_kind(right))
        .then_with(|| left.revision().path().cmp(right.revision().path()))
        .then_with(|| {
            left.revision()
                .content_hash()
                .cmp(&right.revision().content_hash())
        })
        .then_with(|| evidence_range(left).cmp(&evidence_range(right)))
}

const fn evidence_kind(evidence: &CommandEvidence) -> u8 {
    match evidence {
        CommandEvidence::File(_) => 0,
        CommandEvidence::Source(_) => 1,
    }
}

const fn evidence_range(evidence: &CommandEvidence) -> Option<super::SourceRange> {
    match evidence {
        CommandEvidence::File(_) => None,
        CommandEvidence::Source(evidence) => Some(evidence.range()),
    }
}

fn derive_command_id(
    kind: DiscoveredCommandKind,
    working_directory: &WorkspaceDirectory,
    executable: &ProcessExecutable,
    arguments: &[ProcessArgument],
    evidence: &[CommandEvidence],
) -> DiscoveredCommandId {
    let mut hasher = blake3::Hasher::new_derive_key("a3.discovered-command.v1");
    hasher.update(&[kind_code(kind)]);
    hash_directory(&mut hasher, working_directory);
    hash_bytes(&mut hasher, executable.as_str().as_bytes());
    hasher.update(&(arguments.len() as u64).to_le_bytes());
    for argument in arguments {
        hash_bytes(&mut hasher, argument.as_str().as_bytes());
    }
    hasher.update(&(evidence.len() as u64).to_le_bytes());
    for item in evidence {
        hasher.update(&[evidence_kind(item)]);
        hash_bytes(&mut hasher, item.revision().path().as_bytes());
        hasher.update(item.revision().content_hash().as_bytes());
        if let Some(range) = evidence_range(item) {
            hasher.update(&range.start_byte().to_le_bytes());
            hasher.update(&range.end_byte().to_le_bytes());
            hasher.update(&range.start_position().row().to_le_bytes());
            hasher.update(&range.start_position().column().to_le_bytes());
            hasher.update(&range.end_position().row().to_le_bytes());
            hasher.update(&range.end_position().column().to_le_bytes());
        }
    }
    DiscoveredCommandId::from_bytes(*hasher.finalize().as_bytes())
}

fn derive_catalog_id(
    schema: CommandDiscoverySchemaVersion,
    worktree_id: WorktreeId,
    commands: &[DiscoveredCommand],
) -> CommandCatalogId {
    let mut hasher = blake3::Hasher::new_derive_key("a3.command-catalog.v1");
    hasher.update(&[match schema {
        CommandDiscoverySchemaVersion::V1 => 1,
    }]);
    hasher.update(worktree_id.as_bytes());
    hasher.update(&(commands.len() as u64).to_le_bytes());
    for command in commands {
        hasher.update(command.id.as_bytes());
    }
    CommandCatalogId::from_bytes(*hasher.finalize().as_bytes())
}

const fn kind_code(kind: DiscoveredCommandKind) -> u8 {
    match kind {
        DiscoveredCommandKind::Test => 0,
        DiscoveredCommandKind::Build => 1,
        DiscoveredCommandKind::Lint => 2,
        DiscoveredCommandKind::Format => 3,
    }
}

fn hash_directory(hasher: &mut blake3::Hasher, directory: &WorkspaceDirectory) {
    match directory {
        WorkspaceDirectory::Root => {
            hasher.update(&[0]);
        }
        WorkspaceDirectory::Subtree(path) => {
            hasher.update(&[1]);
            hash_bytes(hasher, path.as_bytes());
        }
    }
}

fn hash_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContentHash, RepositoryPath};

    #[test]
    fn preview_is_not_automatic_until_exact_catalog_is_confirmed_and_bound()
    -> Result<(), Box<dyn Error>> {
        let worktree_id = WorktreeId::from_bytes([1; 32]);
        let command = command("Cargo.toml", [2; 32])?;
        let command_id = command.id();
        let catalog = ProjectCommandCatalog::new(
            CommandDiscoverySchemaVersion::V1,
            worktree_id,
            vec![command],
        )?;
        let preview = catalog.preview(AgentRunId::from_bytes([3; 32]), command_id)?;
        assert_eq!(preview.plan_binding(), ProcessPlanBinding::Unbound);

        let allowlist = ProjectCommandAllowlist::confirm(
            &catalog,
            vec![command_id],
            AgentRunTimestamp::from_unix_millis(4)?,
        )?;
        let bound = catalog.bind_confirmed(
            &allowlist,
            AgentRunId::from_bytes([3; 32]),
            TaskStepId::from_bytes([5; 32]),
            command_id,
        )?;
        assert_eq!(
            bound.plan_binding(),
            ProcessPlanBinding::Validated(TaskStepId::from_bytes([5; 32]))
        );
        assert_eq!(bound.network(), ProcessNetworkScope::Denied);
        Ok(())
    }

    #[test]
    fn manifest_revision_change_stales_the_confirmation() -> Result<(), Box<dyn Error>> {
        let worktree_id = WorktreeId::from_bytes([1; 32]);
        let first = command("Cargo.toml", [2; 32])?;
        let first_id = first.id();
        let first_catalog = ProjectCommandCatalog::new(
            CommandDiscoverySchemaVersion::V1,
            worktree_id,
            vec![first],
        )?;
        let allowlist = ProjectCommandAllowlist::confirm(
            &first_catalog,
            vec![first_id],
            AgentRunTimestamp::from_unix_millis(4)?,
        )?;
        let changed = command("Cargo.toml", [9; 32])?;
        let changed_id = changed.id();
        let changed_catalog = ProjectCommandCatalog::new(
            CommandDiscoverySchemaVersion::V1,
            worktree_id,
            vec![changed],
        )?;
        assert!(matches!(
            changed_catalog.bind_confirmed(
                &allowlist,
                AgentRunId::from_bytes([3; 32]),
                TaskStepId::from_bytes([5; 32]),
                changed_id,
            ),
            Err(DiscoveredCommandProcessError::StaleAllowlist)
        ));
        Ok(())
    }

    fn command(path: &str, hash: [u8; 32]) -> Result<DiscoveredCommand, Box<dyn Error>> {
        Ok(DiscoveredCommand::try_new(
            DiscoveredCommandKind::Test,
            WorkspaceDirectory::Root,
            "cargo".to_owned(),
            vec![
                "test".to_owned(),
                "--offline".to_owned(),
                "--locked".to_owned(),
            ],
            vec![CommandEvidence::File(FileRevision::new(
                RepositoryPath::try_from_bytes(path.as_bytes().to_vec())?,
                ContentHash::from_bytes(hash),
            ))],
        )?)
    }
}
