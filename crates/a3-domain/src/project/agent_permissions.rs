use super::{
    GitPolicyOperation, PathPolicyOperation, PolicyAction, PolicyDisposition, ProcessExecutionMode,
    ProcessNetworkScope, ProcessPlanBinding,
};
use std::{error::Error, fmt};

/// User-selected app-wide execution authority, independent of conversation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentPermissionMode {
    /// Reads are automatic; changes and processes require exact approval.
    #[default]
    AskPermissions,
    /// Classified non-destructive actions are automatic; unknown effects still require approval.
    FullMachine,
}

impl AgentPermissionMode {
    /// Stable storage and IPC representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AskPermissions => "askPermissions",
            Self::FullMachine => "fullMachine",
        }
    }
}

/// Monotone revision of explicitly selected execution permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AgentPermissionRevision(u64);

impl AgentPermissionRevision {
    /// Safe initial revision for new and migrated installations.
    pub const INITIAL: Self = Self(1);

    /// Rejects zero and revisions outside the persistent integer range.
    pub const fn new(value: u64) -> Result<Self, AgentPermissionError> {
        if value == 0 || value > i64::MAX as u64 {
            Err(AgentPermissionError)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the positive revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Advances without wrapping or exceeding persistent limits.
    pub const fn next(self) -> Result<Self, AgentPermissionError> {
        Self::new(self.0.saturating_add(1))
    }
}

/// Content-free permission snapshot retained with an action's policy decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentPermissionSettings {
    mode: AgentPermissionMode,
    revision: AgentPermissionRevision,
}

impl AgentPermissionSettings {
    /// Safe initial state; migration never implicitly opts into machine access.
    pub const INITIAL: Self = Self::new(
        AgentPermissionMode::AskPermissions,
        AgentPermissionRevision::INITIAL,
    );

    /// Reconstructs a validated permission snapshot.
    #[must_use]
    pub const fn new(mode: AgentPermissionMode, revision: AgentPermissionRevision) -> Self {
        Self { mode, revision }
    }

    /// Returns the selected mode.
    #[must_use]
    pub const fn mode(self) -> AgentPermissionMode {
        self.mode
    }

    /// Returns the exact settings revision authorizing an action.
    #[must_use]
    pub const fn revision(self) -> AgentPermissionRevision {
        self.revision
    }

    /// Classifies the complete typed action, preserving destructive composite effects.
    /// Open processes and shell interpretation are deliberately never inferred to be safe.
    #[must_use]
    pub const fn disposition(self, action: &PolicyAction) -> PolicyDisposition {
        let automatic = match action {
            PolicyAction::Root { .. } => true,
            PolicyAction::MachineFile { operation, .. } => {
                matches!(self.mode, AgentPermissionMode::FullMachine)
                    && !matches!(operation, PathPolicyOperation::Delete)
            }
            PolicyAction::Path { scope, operation } => {
                !matches!(operation, PathPolicyOperation::Delete)
                    && (matches!(self.mode, AgentPermissionMode::FullMachine)
                        || matches!(
                            (scope, operation),
                            (
                                super::PolicyPathScope::Worktree { .. },
                                PathPolicyOperation::Read
                            )
                        ))
            }
            PolicyAction::Patch(patch) => {
                matches!(self.mode, AgentPermissionMode::FullMachine) && !patch.destructive()
            }
            PolicyAction::MachineProcess { process, effect } => {
                matches!(self.mode, AgentPermissionMode::FullMachine)
                    && matches!(effect, super::MachineProcessEffect::ReadOnly)
                    && matches!(process.mode(), ProcessExecutionMode::KnownSafe)
                    && matches!(process.plan_binding(), ProcessPlanBinding::Validated(_))
                    && matches!(process.network(), ProcessNetworkScope::Denied)
            }
            PolicyAction::Process(process) => {
                matches!(self.mode, AgentPermissionMode::FullMachine)
                    && matches!(process.mode(), ProcessExecutionMode::KnownSafe)
                    && matches!(process.plan_binding(), ProcessPlanBinding::Validated(_))
                    && matches!(process.network(), ProcessNetworkScope::Denied)
            }
            PolicyAction::MachineHttpGet { .. } => {
                matches!(self.mode, AgentPermissionMode::FullMachine)
            }
            PolicyAction::Network { .. } => false,
            PolicyAction::Git { operation, .. } => match operation {
                GitPolicyOperation::Status
                | GitPolicyOperation::Diff
                | GitPolicyOperation::Log
                | GitPolicyOperation::Show
                | GitPolicyOperation::RevParse
                | GitPolicyOperation::ListFiles => true,
                GitPolicyOperation::Commit
                | GitPolicyOperation::CreateBranch
                | GitPolicyOperation::Fetch => {
                    matches!(self.mode, AgentPermissionMode::FullMachine)
                }
                _ => false,
            },
        };
        if automatic {
            PolicyDisposition::Automatic
        } else {
            PolicyDisposition::ApprovalRequired
        }
    }
}

/// Invalid or exhausted permission revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentPermissionError;
impl fmt::Display for AgentPermissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid agent permission revision")
    }
}
impl Error for AgentPermissionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PathScopeCoverage, PolicyPathScope, PolicyResourceId, ProcessPolicyAction, RepositoryPath,
        TaskStepId, WorktreeId,
    };

    #[test]
    fn modes_preserve_unknown_destructive_and_external_boundaries() -> Result<(), Box<dyn Error>> {
        let worktree_id = WorktreeId::from_bytes([1; 32]);
        let scope = PolicyPathScope::Worktree {
            worktree_id,
            path: RepositoryPath::try_from_bytes(b"server.py".to_vec())?,
            coverage: PathScopeCoverage::Exact,
        };
        let full = AgentPermissionSettings::new(
            AgentPermissionMode::FullMachine,
            AgentPermissionRevision::new(2)?,
        );
        for (scope, operation, ask, automatic) in [
            (scope.clone(), PathPolicyOperation::Read, true, true),
            (scope.clone(), PathPolicyOperation::Write, false, true),
            (scope, PathPolicyOperation::Delete, false, false),
            (
                PolicyPathScope::OutsideRoot {
                    resource_id: PolicyResourceId::from_bytes([2; 32]),
                },
                PathPolicyOperation::Read,
                false,
                true,
            ),
            (
                PolicyPathScope::OutsideRoot {
                    resource_id: PolicyResourceId::from_bytes([2; 32]),
                },
                PathPolicyOperation::Delete,
                false,
                false,
            ),
        ] {
            let action = PolicyAction::Path { scope, operation };
            assert_eq!(
                AgentPermissionSettings::INITIAL.disposition(&action)
                    == PolicyDisposition::Automatic,
                ask
            );
            assert_eq!(
                full.disposition(&action) == PolicyDisposition::Automatic,
                automatic
            );
        }
        for (mode, automatic) in [
            (ProcessExecutionMode::KnownSafe, true),
            (ProcessExecutionMode::Open, false),
            (ProcessExecutionMode::Shell, false),
        ] {
            let process = PolicyAction::Process(ProcessPolicyAction::new(
                worktree_id,
                PolicyResourceId::from_bytes([3; 32]),
                mode,
                ProcessPlanBinding::Validated(TaskStepId::from_bytes([4; 32])),
                ProcessNetworkScope::Denied,
            ));
            assert_eq!(
                AgentPermissionSettings::INITIAL.disposition(&process),
                PolicyDisposition::ApprovalRequired
            );
            assert_eq!(
                full.disposition(&process) == PolicyDisposition::Automatic,
                automatic
            );
        }
        for operation in [
            GitPolicyOperation::Push,
            GitPolicyOperation::Clean,
            GitPolicyOperation::CheckoutWithLoss,
            GitPolicyOperation::Reset,
            GitPolicyOperation::Merge,
        ] {
            assert_eq!(
                full.disposition(&PolicyAction::Git {
                    worktree_id,
                    operation
                }),
                PolicyDisposition::ApprovalRequired
            );
        }
        for mode in [
            ProcessExecutionMode::KnownSafe,
            ProcessExecutionMode::Open,
            ProcessExecutionMode::Shell,
        ] {
            let action = PolicyAction::Process(ProcessPolicyAction::new(
                worktree_id,
                PolicyResourceId::from_bytes([8; 32]),
                mode,
                ProcessPlanBinding::Validated(TaskStepId::from_bytes([9; 32])),
                ProcessNetworkScope::Requested(PolicyResourceId::from_bytes([10; 32])),
            ));
            assert_eq!(
                full.disposition(&action),
                PolicyDisposition::ApprovalRequired
            );
            assert!(action.class().permits_risk(action.risk()));
            if mode == ProcessExecutionMode::Shell {
                assert_eq!(action.class(), crate::ActionClass::ExecuteOpen);
                assert_eq!(
                    action.additional_restrictions()[0],
                    Some(crate::ActionClass::Network)
                );
            }
        }
        Ok(())
    }

    #[test]
    fn revisions_never_wrap_or_accept_zero() -> Result<(), AgentPermissionError> {
        assert!(AgentPermissionRevision::new(0).is_err());
        assert!(
            AgentPermissionRevision::new(i64::MAX as u64)?
                .next()
                .is_err()
        );
        assert_eq!(AgentPermissionRevision::INITIAL.next()?.get(), 2);
        Ok(())
    }
}
