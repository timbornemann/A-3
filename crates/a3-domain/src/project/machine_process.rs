use crate::{ProcessArgument, ProcessExecutable, TaskStepId};
use std::{error::Error, fmt};

/// Closed Core classification of an additional process. Models cannot submit this field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MachineProcessEffect {
    /// Exact Core-owned version query with an empty ambient environment.
    ReadOnly,
    /// Program, script or parameter effects were not established by a closed Core rule.
    Unknown,
    /// Recognized deletion or irreversible local-state change.
    Destructive,
    /// Recognized push, release, remote write or publication.
    Publish,
}

/// V1 additional direct-argv proposal; Core owns CWD, environment, limits and classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineProcessAction {
    step: TaskStepId,
    executable: ProcessExecutable,
    arguments: Vec<ProcessArgument>,
}
impl MachineProcessAction {
    /// Bounds a proposal without granting process, shell or network authority.
    pub fn new(
        step: TaskStepId,
        executable: ProcessExecutable,
        arguments: Vec<ProcessArgument>,
    ) -> Result<Self, MachineProcessActionError> {
        if arguments.len() > 32
            || arguments
                .iter()
                .map(|a| a.as_str().len())
                .sum::<usize>()
                .saturating_add(executable.as_str().len())
                > 8192
        {
            return Err(MachineProcessActionError);
        }
        Ok(Self {
            step,
            executable,
            arguments,
        })
    }
    /// Owning active ledger step, independently validated by Core.
    #[must_use]
    pub const fn step_id(&self) -> TaskStepId {
        self.step
    }
    /// Exact untrusted executable token.
    #[must_use]
    pub const fn executable(&self) -> &ProcessExecutable {
        &self.executable
    }
    /// Exact bounded arguments, never interpreted as a shell command.
    #[must_use]
    pub fn arguments(&self) -> &[ProcessArgument] {
        &self.arguments
    }
}

/// Invalid additional-process proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineProcessActionError;
impl fmt::Display for MachineProcessActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid machine process proposal")
    }
}
impl Error for MachineProcessActionError {}
