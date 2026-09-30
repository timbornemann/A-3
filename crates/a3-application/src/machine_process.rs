use a3_domain::{
    AgentControllerState, AgentRun, MachineProcessAction, MachineProcessEffect, PolicyResourceId,
    ProcessEnvironmentVariable, ProcessExecutionMode, ProcessNetworkScope, ProcessOutputLimit,
    ProcessPlanBinding, ProcessSpec, ProcessSpecSchemaVersion, ProcessTimeout, ProjectIdentity,
    TaskLedger, TaskStepStatus, WorkspaceDirectory,
};
use std::{error::Error, fmt};

/// Prepares additional direct processes through closed Core rules, never model-authored flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct PrepareMachineProcess;
impl PrepareMachineProcess {
    /// Revalidates the active plan and uses only the composition root's environment allowlist.
    pub fn execute(
        self,
        project: &ProjectIdentity,
        run: &AgentRun,
        ledger: &TaskLedger,
        action: &MachineProcessAction,
        environment: &[ProcessEnvironmentVariable],
    ) -> Result<ProcessSpec, MachineProcessPreparationError> {
        if !matches!(
            run.state(),
            AgentControllerState::Execute | AgentControllerState::AwaitApproval
        ) || run.goal_contract() != ledger.goal_contract()
            || run.task_ledger_revision() != ledger.revision()
            || !ledger.step(action.step_id()).is_some_and(|s| {
                matches!(
                    s.status(),
                    TaskStepStatus::InProgress | TaskStepStatus::AwaitingApproval
                )
            })
        {
            return Err(MachineProcessPreparationError);
        }
        let effect = classify_machine_process(action);
        let mode = if effect == MachineProcessEffect::ReadOnly {
            ProcessExecutionMode::KnownSafe
        } else {
            ProcessExecutionMode::Open
        };
        let network = if effect == MachineProcessEffect::ReadOnly {
            ProcessNetworkScope::Denied
        } else {
            // Unknown code can communicate beyond the project. This marks uncertainty, not
            // OS network isolation or a fabricated known destination.
            let mut hash = blake3::Hasher::new_derive_key("a3.machine-process-unknown-network.v1");
            hash.update(action.executable().as_str().as_bytes());
            for arg in action.arguments() {
                hash.update(&(arg.as_str().len() as u64).to_le_bytes());
                hash.update(arg.as_str().as_bytes());
            }
            ProcessNetworkScope::Requested(PolicyResourceId::from_bytes(
                *hash.finalize().as_bytes(),
            ))
        };
        let spec = ProcessSpec::new(
            ProcessSpecSchemaVersion::V2,
            run.id(),
            project.worktree().id(),
            action.executable().clone(),
            action.arguments().to_vec(),
            WorkspaceDirectory::Root,
            environment.to_vec(),
            ProcessTimeout::from_millis(30_000).map_err(|_| MachineProcessPreparationError)?,
            ProcessOutputLimit::new(16_384).map_err(|_| MachineProcessPreparationError)?,
            ProcessOutputLimit::new(16_384).map_err(|_| MachineProcessPreparationError)?,
            mode,
            ProcessPlanBinding::Validated(action.step_id()),
            network,
        )
        .map_err(|_| MachineProcessPreparationError)?
        .with_machine_effect(effect);
        Ok(if is_shell(action.executable().as_str()) {
            spec.with_explicit_machine_shell()
        } else {
            spec
        })
    }
}

/// Closed parameter rules. Absolute/path-qualified programs and additional flags are unknown.
#[must_use]
pub fn classify_machine_process(action: &MachineProcessAction) -> MachineProcessEffect {
    let program = action.executable().as_str().to_ascii_lowercase();
    let program = program.strip_suffix(".exe").unwrap_or(&program);
    let arguments = action
        .arguments()
        .iter()
        .map(|a| a.as_str())
        .collect::<Vec<_>>();
    if matches!(program, "python" | "python3" | "git" | "node") && arguments == ["--version"] {
        return MachineProcessEffect::ReadOnly;
    }
    if matches!(program, "git" | "npm" | "pnpm" | "cargo")
        && arguments
            .first()
            .is_some_and(|a| matches!(*a, "push" | "publish" | "release"))
    {
        return MachineProcessEffect::Publish;
    }
    if matches!(program, "rm" | "rmdir" | "del" | "erase")
        || (program == "git"
            && arguments
                .first()
                .is_some_and(|a| matches!(*a, "reset" | "clean" | "rebase" | "merge")))
    {
        return MachineProcessEffect::Destructive;
    }
    MachineProcessEffect::Unknown
}
fn is_shell(program: &str) -> bool {
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .to_ascii_lowercase();
    matches!(
        name.strip_suffix(".exe").unwrap_or(&name),
        "cmd" | "powershell" | "pwsh" | "sh" | "bash" | "zsh" | "fish"
    )
}

/// Content-free anchor, parameter or resource-bound failure before any process starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineProcessPreparationError;
impl fmt::Display for MachineProcessPreparationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("machine process preparation denied")
    }
}
impl Error for MachineProcessPreparationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use a3_domain::{ProcessArgument, ProcessExecutable, TaskStepId};
    #[test]
    fn additional_programs_and_parameters_cannot_claim_safe_effects() -> Result<(), Box<dyn Error>>
    {
        for (program, args, expected) in [
            ("python", vec!["--version"], MachineProcessEffect::ReadOnly),
            ("python", vec!["server.py"], MachineProcessEffect::Unknown),
            (
                "python",
                vec!["--version", "--extra"],
                MachineProcessEffect::Unknown,
            ),
            (
                "D:/untrusted/python.exe",
                vec!["--version"],
                MachineProcessEffect::Unknown,
            ),
            (
                "git",
                vec!["push", "--delete", "origin", "branch"],
                MachineProcessEffect::Publish,
            ),
            (
                "git",
                vec!["reset", "--hard"],
                MachineProcessEffect::Destructive,
            ),
            (
                "pip",
                vec!["install", "unclassified-package"],
                MachineProcessEffect::Unknown,
            ),
            (
                "cmd",
                vec!["/c", "echo hi & del *"],
                MachineProcessEffect::Unknown,
            ),
        ] {
            let action = MachineProcessAction::new(
                TaskStepId::from_bytes([1; 32]),
                ProcessExecutable::try_from_string(program.to_owned())?,
                args.into_iter()
                    .map(|a| ProcessArgument::try_from_string(a.to_owned()))
                    .collect::<Result<_, _>>()?,
            )?;
            assert_eq!(classify_machine_process(&action), expected);
        }
        Ok(())
    }
}
