use crate::{
    AgentRunId, PolicyAction, PolicyResourceId, ProcessExecutionMode, ProcessNetworkScope,
    ProcessPlanBinding, ProcessPolicyAction, SecretCandidateClassifierV1, WorkspaceDirectory,
    WorktreeId,
};
use std::error::Error;
use std::fmt;

const MAX_EXECUTABLE_BYTES: usize = 4 * 1_024;
const MAX_ARGUMENTS: usize = 256;
const MAX_ARGUMENT_BYTES: usize = 4 * 1_024;
const MAX_TOTAL_ARGV_BYTES: usize = 64 * 1_024;
const MAX_ENVIRONMENT_VARIABLES: usize = 64;
const MAX_ENVIRONMENT_NAME_BYTES: usize = 128;
const MAX_TIMEOUT_MILLIS: u64 = 30 * 60 * 1_000;
const MAX_OUTPUT_BYTES: u32 = 4 * 1_024 * 1_024;

/// Version of the closed, shell-free process specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProcessSpecSchemaVersion {
    /// Initial direct-argv process contract.
    V1,
}

/// Bounded executable name or absolute UTF-8 platform path.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ProcessExecutable(String);

impl ProcessExecutable {
    /// Validates an executable without interpreting it as a command string.
    pub fn try_from_string(value: String) -> Result<Self, ProcessExecutableError> {
        if value.is_empty() || value.len() > MAX_EXECUTABLE_BYTES {
            return Err(ProcessExecutableError::InvalidLength {
                actual: value.len(),
            });
        }
        if value
            .chars()
            .any(|character| character == '\0' || character.is_control())
        {
            return Err(ProcessExecutableError::InvalidCharacter);
        }
        Ok(Self(value))
    }

    /// Returns the exact executable token for adapter-side resolution.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProcessExecutable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessExecutable")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Executable text crossed the fixed direct-process boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessExecutableError {
    /// The executable was empty or exceeded four KiB.
    InvalidLength {
        /// Observed UTF-8 byte count.
        actual: usize,
    },
    /// The executable contained NUL or another control character.
    InvalidCharacter,
}

impl fmt::Display for ProcessExecutableError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLength { .. } => "process executable length is invalid",
            Self::InvalidCharacter => "process executable contains an unsupported character",
        })
    }
}

impl Error for ProcessExecutableError {}

/// One exact argv value; shell metacharacters remain ordinary bytes.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ProcessArgument(String);

impl ProcessArgument {
    /// Validates one argument without trimming, splitting, or shell interpretation.
    pub fn try_from_string(value: String) -> Result<Self, ProcessArgumentError> {
        if value.len() > MAX_ARGUMENT_BYTES {
            return Err(ProcessArgumentError::TooLarge {
                actual: value.len(),
            });
        }
        if value.contains('\0') {
            return Err(ProcessArgumentError::Nul);
        }
        if SecretCandidateClassifierV1::classify(&value).is_some() {
            return Err(ProcessArgumentError::SecretCandidate);
        }
        Ok(Self(value))
    }

    /// Returns the exact argv value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProcessArgument {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessArgument")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Invalid direct argv value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessArgumentError {
    /// The argument exceeded four KiB.
    TooLarge {
        /// Observed UTF-8 byte count.
        actual: usize,
    },
    /// The argument contained NUL, which no supported OS argv can represent.
    Nul,
    /// The argument contained a possible credential.
    SecretCandidate,
}

impl fmt::Display for ProcessArgumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooLarge { .. } => "process argument exceeds four KiB",
            Self::Nul => "process argument contains NUL",
            Self::SecretCandidate => "process argument contains a possible secret",
        })
    }
}

impl Error for ProcessArgumentError {}

/// Canonical cross-platform name of one explicitly admitted host environment variable.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessEnvironmentVariable(String);

impl ProcessEnvironmentVariable {
    /// Accepts only uppercase ASCII names so Windows case-folding cannot create duplicates.
    pub fn try_from_string(value: String) -> Result<Self, ProcessEnvironmentVariableError> {
        if value.is_empty() || value.len() > MAX_ENVIRONMENT_NAME_BYTES {
            return Err(ProcessEnvironmentVariableError::InvalidLength {
                actual: value.len(),
            });
        }
        let mut bytes = value.bytes();
        let first = bytes
            .next()
            .ok_or(ProcessEnvironmentVariableError::InvalidLength { actual: 0 })?;
        if !(first.is_ascii_uppercase() || first == b'_')
            || bytes
                .any(|byte| !(byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'))
        {
            return Err(ProcessEnvironmentVariableError::InvalidName);
        }
        Ok(Self(value))
    }

    /// Returns the canonical variable name; its value remains adapter-owned and redacted.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProcessEnvironmentVariable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessEnvironmentVariable")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Invalid environment-allowlist key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessEnvironmentVariableError {
    /// The name was empty or exceeded 128 bytes.
    InvalidLength {
        /// Observed UTF-8 byte count.
        actual: usize,
    },
    /// The name was not canonical uppercase ASCII identifier syntax.
    InvalidName,
}

impl fmt::Display for ProcessEnvironmentVariableError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLength { .. } => "process environment variable length is invalid",
            Self::InvalidName => "process environment variable name is invalid",
        })
    }
}

impl Error for ProcessEnvironmentVariableError {}

/// Positive process lifetime bound, in monotonic milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessTimeout(u64);

impl ProcessTimeout {
    /// Creates a timeout between one millisecond and thirty minutes.
    pub const fn from_millis(value: u64) -> Result<Self, ProcessTimeoutError> {
        if value == 0 || value > MAX_TIMEOUT_MILLIS {
            return Err(ProcessTimeoutError { value });
        }
        Ok(Self(value))
    }

    /// Returns the stable millisecond representation.
    #[must_use]
    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

/// Process timeout was zero or exceeded thirty minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessTimeoutError {
    value: u64,
}

impl fmt::Display for ProcessTimeoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "process timeout {} is outside 1..={MAX_TIMEOUT_MILLIS} milliseconds",
            self.value
        )
    }
}

impl Error for ProcessTimeoutError {}

/// Positive retained-output cap for exactly one process stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessOutputLimit(u32);

impl ProcessOutputLimit {
    /// Creates a cap between one byte and four MiB.
    pub const fn new(value: u32) -> Result<Self, ProcessOutputLimitError> {
        if value == 0 || value > MAX_OUTPUT_BYTES {
            return Err(ProcessOutputLimitError { value });
        }
        Ok(Self(value))
    }

    /// Returns the stable byte count.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Output cap was zero or exceeded four MiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessOutputLimitError {
    value: u32,
}

impl fmt::Display for ProcessOutputLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "process output limit {} is outside 1..={MAX_OUTPUT_BYTES}",
            self.value
        )
    }
}

impl Error for ProcessOutputLimitError {}

/// Immutable direct-argv process request and its exact central-policy identity.
#[derive(Clone, PartialEq, Eq)]
pub struct ProcessSpec {
    version: ProcessSpecSchemaVersion,
    run_id: AgentRunId,
    worktree_id: WorktreeId,
    executable: ProcessExecutable,
    arguments: Vec<ProcessArgument>,
    working_directory: WorkspaceDirectory,
    environment_allowlist: Vec<ProcessEnvironmentVariable>,
    timeout: ProcessTimeout,
    stdout_limit: ProcessOutputLimit,
    stderr_limit: ProcessOutputLimit,
    execution_mode: ProcessExecutionMode,
    plan_binding: ProcessPlanBinding,
    network: ProcessNetworkScope,
    specification_id: PolicyResourceId,
}

impl ProcessSpec {
    /// Canonicalizes the environment allowlist and rejects an implicit shell contract.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: ProcessSpecSchemaVersion,
        run_id: AgentRunId,
        worktree_id: WorktreeId,
        executable: ProcessExecutable,
        arguments: Vec<ProcessArgument>,
        working_directory: WorkspaceDirectory,
        mut environment_allowlist: Vec<ProcessEnvironmentVariable>,
        timeout: ProcessTimeout,
        stdout_limit: ProcessOutputLimit,
        stderr_limit: ProcessOutputLimit,
        execution_mode: ProcessExecutionMode,
        plan_binding: ProcessPlanBinding,
        network: ProcessNetworkScope,
    ) -> Result<Self, ProcessSpecError> {
        if execution_mode == ProcessExecutionMode::Shell {
            return Err(ProcessSpecError::ShellUnsupported);
        }
        if arguments.len() > MAX_ARGUMENTS {
            return Err(ProcessSpecError::TooManyArguments {
                actual: arguments.len(),
            });
        }
        let argv_bytes =
            arguments
                .iter()
                .try_fold(executable.as_str().len(), |total, argument| {
                    total
                        .checked_add(argument.as_str().len())
                        .and_then(|sum| sum.checked_add(1))
                        .ok_or(ProcessSpecError::ArgvTooLarge)
                })?;
        if argv_bytes > MAX_TOTAL_ARGV_BYTES {
            return Err(ProcessSpecError::ArgvTooLarge);
        }
        if environment_allowlist.len() > MAX_ENVIRONMENT_VARIABLES {
            return Err(ProcessSpecError::TooManyEnvironmentVariables {
                actual: environment_allowlist.len(),
            });
        }
        environment_allowlist.sort();
        if environment_allowlist
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        {
            return Err(ProcessSpecError::DuplicateEnvironmentVariable);
        }
        let specification_id = derive_specification_id(
            version,
            run_id,
            worktree_id,
            &executable,
            &arguments,
            &working_directory,
            &environment_allowlist,
            timeout,
            stdout_limit,
            stderr_limit,
            execution_mode,
            plan_binding,
            network,
        );
        Ok(Self {
            version,
            run_id,
            worktree_id,
            executable,
            arguments,
            working_directory,
            environment_allowlist,
            timeout,
            stdout_limit,
            stderr_limit,
            execution_mode,
            plan_binding,
            network,
            specification_id,
        })
    }

    /// Returns the schema version.
    #[must_use]
    pub const fn version(&self) -> ProcessSpecSchemaVersion {
        self.version
    }

    /// Returns the owning run.
    #[must_use]
    pub const fn run_id(&self) -> AgentRunId {
        self.run_id
    }

    /// Returns the exact worktree boundary.
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    /// Returns the un-interpreted executable token.
    #[must_use]
    pub const fn executable(&self) -> &ProcessExecutable {
        &self.executable
    }

    /// Returns the exact ordered argv tail.
    #[must_use]
    pub fn arguments(&self) -> &[ProcessArgument] {
        &self.arguments
    }

    /// Returns the worktree-relative CWD selection.
    #[must_use]
    pub const fn working_directory(&self) -> &WorkspaceDirectory {
        &self.working_directory
    }

    /// Returns canonical names whose adapter-provided values may enter the child.
    #[must_use]
    pub fn environment_allowlist(&self) -> &[ProcessEnvironmentVariable] {
        &self.environment_allowlist
    }

    /// Returns the positive total timeout.
    #[must_use]
    pub const fn timeout(&self) -> ProcessTimeout {
        self.timeout
    }

    /// Returns the retained stdout cap.
    #[must_use]
    pub const fn stdout_limit(&self) -> ProcessOutputLimit {
        self.stdout_limit
    }

    /// Returns the retained stderr cap.
    #[must_use]
    pub const fn stderr_limit(&self) -> ProcessOutputLimit {
        self.stderr_limit
    }

    /// Returns whether policy treats the direct process as known-safe or open.
    #[must_use]
    pub const fn execution_mode(&self) -> ProcessExecutionMode {
        self.execution_mode
    }

    /// Returns the validated-plan binding used by central policy.
    #[must_use]
    pub const fn plan_binding(&self) -> ProcessPlanBinding {
        self.plan_binding
    }

    /// Returns the declared network scope; the runner does not claim OS sandboxing.
    #[must_use]
    pub const fn network(&self) -> ProcessNetworkScope {
        self.network
    }

    /// Returns the exact content-free specification identity.
    #[must_use]
    pub const fn specification_id(&self) -> PolicyResourceId {
        self.specification_id
    }

    /// Projects this exact bounded specification into the central policy engine.
    #[must_use]
    pub fn policy_action(&self) -> PolicyAction {
        PolicyAction::Process(ProcessPolicyAction::new(
            self.worktree_id,
            self.specification_id,
            self.execution_mode,
            self.plan_binding,
            self.network,
        ))
    }
}

impl fmt::Debug for ProcessSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessSpec")
            .field("version", &self.version)
            .field("run_id", &self.run_id)
            .field("worktree_id", &self.worktree_id)
            .field("executable", &self.executable)
            .field("argument_count", &self.arguments.len())
            .field("working_directory", &self.working_directory)
            .field("environment_count", &self.environment_allowlist.len())
            .field("timeout", &self.timeout)
            .field("stdout_limit", &self.stdout_limit)
            .field("stderr_limit", &self.stderr_limit)
            .field("execution_mode", &self.execution_mode)
            .field("plan_binding", &self.plan_binding)
            .field("network", &self.network)
            .field("specification_id", &self.specification_id)
            .finish()
    }
}

/// Process specification violated a shell, argv, or environment invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessSpecError {
    /// Shell execution is a separate action and cannot enter the direct runner.
    ShellUnsupported,
    /// More than 256 arguments were supplied.
    TooManyArguments {
        /// Observed argument count.
        actual: usize,
    },
    /// Executable plus argv exceeded 64 KiB.
    ArgvTooLarge,
    /// More than 64 environment names were requested.
    TooManyEnvironmentVariables {
        /// Observed environment-name count.
        actual: usize,
    },
    /// The case-canonical allowlist contained a duplicate.
    DuplicateEnvironmentVariable,
}

impl fmt::Display for ProcessSpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ShellUnsupported => "direct process specification cannot enable shell mode",
            Self::TooManyArguments { .. } => "process specification exceeds 256 arguments",
            Self::ArgvTooLarge => "process argv exceeds 64 KiB",
            Self::TooManyEnvironmentVariables { .. } => {
                "process specification exceeds 64 environment variables"
            }
            Self::DuplicateEnvironmentVariable => {
                "process environment allowlist contains a duplicate"
            }
        })
    }
}

impl Error for ProcessSpecError {}

#[allow(clippy::too_many_arguments)]
fn derive_specification_id(
    version: ProcessSpecSchemaVersion,
    run_id: AgentRunId,
    worktree_id: WorktreeId,
    executable: &ProcessExecutable,
    arguments: &[ProcessArgument],
    working_directory: &WorkspaceDirectory,
    environment_allowlist: &[ProcessEnvironmentVariable],
    timeout: ProcessTimeout,
    stdout_limit: ProcessOutputLimit,
    stderr_limit: ProcessOutputLimit,
    execution_mode: ProcessExecutionMode,
    plan_binding: ProcessPlanBinding,
    network: ProcessNetworkScope,
) -> PolicyResourceId {
    let mut hasher = blake3::Hasher::new_derive_key("a3.process-spec.v1");
    hasher.update(&[match version {
        ProcessSpecSchemaVersion::V1 => 1,
    }]);
    hasher.update(run_id.as_bytes());
    hasher.update(worktree_id.as_bytes());
    hash_bytes(&mut hasher, executable.as_str().as_bytes());
    hasher.update(&(arguments.len() as u64).to_le_bytes());
    for argument in arguments {
        hash_bytes(&mut hasher, argument.as_str().as_bytes());
    }
    match working_directory {
        WorkspaceDirectory::Root => {
            hasher.update(&[0]);
        }
        WorkspaceDirectory::Subtree(path) => {
            hasher.update(&[1]);
            hash_bytes(&mut hasher, path.as_bytes());
        }
    };
    hasher.update(&(environment_allowlist.len() as u64).to_le_bytes());
    for variable in environment_allowlist {
        hash_bytes(&mut hasher, variable.as_str().as_bytes());
    }
    hasher.update(&timeout.as_millis().to_le_bytes());
    hasher.update(&stdout_limit.get().to_le_bytes());
    hasher.update(&stderr_limit.get().to_le_bytes());
    hasher.update(&[match execution_mode {
        ProcessExecutionMode::KnownSafe => 0,
        ProcessExecutionMode::Open => 1,
        ProcessExecutionMode::Shell => 2,
    }]);
    match plan_binding {
        ProcessPlanBinding::Unbound => {
            hasher.update(&[0]);
        }
        ProcessPlanBinding::Validated(step_id) => {
            hasher.update(&[1]);
            hasher.update(step_id.as_bytes());
        }
    };
    match network {
        ProcessNetworkScope::Denied => {
            hasher.update(&[0]);
        }
        ProcessNetworkScope::Requested(resource_id) => {
            hasher.update(&[1]);
            hasher.update(resource_id.as_bytes());
        }
    };
    PolicyResourceId::from_bytes(*hasher.finalize().as_bytes())
}

fn hash_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionClass, RiskLevel, SystemPolicyV1, TaskStepId};

    #[test]
    fn shell_characters_remain_one_argument_and_change_the_exact_digest()
    -> Result<(), Box<dyn Error>> {
        let argument = ProcessArgument::try_from_string("$(touch nope); && | >".to_owned())?;
        assert_eq!(argument.as_str(), "$(touch nope); && | >");
        let first = spec(vec![argument])?;
        let second = spec(vec![ProcessArgument::try_from_string(
            "different".to_owned(),
        )?])?;
        assert_ne!(first.specification_id(), second.specification_id());
        assert_eq!(first.policy_action().class(), ActionClass::ExecuteSafe);
        assert_eq!(first.policy_action().risk(), RiskLevel::Moderate);
        assert_eq!(
            SystemPolicyV1.disposition(&first.policy_action()),
            crate::PolicyDisposition::Automatic
        );
        Ok(())
    }

    #[test]
    fn environment_is_canonical_and_shell_mode_is_not_constructible() -> Result<(), Box<dyn Error>>
    {
        let path = ProcessEnvironmentVariable::try_from_string("PATH".to_owned())?;
        let temp = ProcessEnvironmentVariable::try_from_string("TEMP".to_owned())?;
        let configured = ProcessSpec::new(
            ProcessSpecSchemaVersion::V1,
            AgentRunId::from_bytes([1; 32]),
            WorktreeId::from_bytes([2; 32]),
            ProcessExecutable::try_from_string("cargo".to_owned())?,
            Vec::new(),
            WorkspaceDirectory::Root,
            vec![temp, path],
            ProcessTimeout::from_millis(1_000)?,
            ProcessOutputLimit::new(1_024)?,
            ProcessOutputLimit::new(1_024)?,
            ProcessExecutionMode::KnownSafe,
            ProcessPlanBinding::Validated(TaskStepId::from_bytes([3; 32])),
            ProcessNetworkScope::Denied,
        )?;
        assert_eq!(configured.environment_allowlist()[0].as_str(), "PATH");
        assert_eq!(configured.environment_allowlist()[1].as_str(), "TEMP");

        assert_eq!(
            ProcessSpec::new(
                ProcessSpecSchemaVersion::V1,
                AgentRunId::from_bytes([1; 32]),
                WorktreeId::from_bytes([2; 32]),
                ProcessExecutable::try_from_string("cargo".to_owned())?,
                Vec::new(),
                WorkspaceDirectory::Root,
                Vec::new(),
                ProcessTimeout::from_millis(1_000)?,
                ProcessOutputLimit::new(1_024)?,
                ProcessOutputLimit::new(1_024)?,
                ProcessExecutionMode::Shell,
                ProcessPlanBinding::Unbound,
                ProcessNetworkScope::Denied,
            ),
            Err(ProcessSpecError::ShellUnsupported)
        );
        Ok(())
    }

    #[test]
    fn network_request_is_classified_and_never_automatic() -> Result<(), Box<dyn Error>> {
        let process = ProcessSpec::new(
            ProcessSpecSchemaVersion::V1,
            AgentRunId::from_bytes([1; 32]),
            WorktreeId::from_bytes([2; 32]),
            ProcessExecutable::try_from_string("cargo".to_owned())?,
            Vec::new(),
            WorkspaceDirectory::Root,
            Vec::new(),
            ProcessTimeout::from_millis(1_000)?,
            ProcessOutputLimit::new(1_024)?,
            ProcessOutputLimit::new(1_024)?,
            ProcessExecutionMode::KnownSafe,
            ProcessPlanBinding::Validated(TaskStepId::from_bytes([3; 32])),
            ProcessNetworkScope::Requested(PolicyResourceId::from_bytes([4; 32])),
        )?;
        assert_eq!(process.policy_action().class(), ActionClass::Network);
        assert_eq!(
            SystemPolicyV1.disposition(&process.policy_action()),
            crate::PolicyDisposition::ApprovalRequired
        );
        Ok(())
    }

    fn spec(arguments: Vec<ProcessArgument>) -> Result<ProcessSpec, Box<dyn Error>> {
        Ok(ProcessSpec::new(
            ProcessSpecSchemaVersion::V1,
            AgentRunId::from_bytes([1; 32]),
            WorktreeId::from_bytes([2; 32]),
            ProcessExecutable::try_from_string("cargo".to_owned())?,
            arguments,
            WorkspaceDirectory::Root,
            Vec::new(),
            ProcessTimeout::from_millis(1_000)?,
            ProcessOutputLimit::new(1_024)?,
            ProcessOutputLimit::new(1_024)?,
            ProcessExecutionMode::KnownSafe,
            ProcessPlanBinding::Validated(TaskStepId::from_bytes([3; 32])),
            ProcessNetworkScope::Denied,
        )?)
    }
}
