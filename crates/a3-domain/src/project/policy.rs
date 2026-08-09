use super::{PatchPolicyAction, PolicyResourceId, RepositoryPath, TaskStepId, WorktreeId};
use std::error::Error;
use std::fmt;

const POLICY_ACTION_DIGEST_DOMAIN: &str = "a3.policy-action.v1";
const POLICY_SCOPE_DIGEST_DOMAIN: &str = "a3.policy-scope.v1";
const MAX_WORKSPACE_POLICY_RULES: usize = 9;

/// Security-relevant action class derived from a typed action, never supplied independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActionClass {
    /// Bounded observation inside an approved worktree root.
    Read,
    /// Deterministic local derivation without a privileged external side effect.
    Derive,
    /// One bounded local workspace mutation.
    Write,
    /// A known argv-based command with no network and a validated plan binding.
    ExecuteSafe,
    /// An arbitrary argv process or explicit shell mode.
    ExecuteOpen,
    /// Any action that can communicate outside the local process boundary.
    Network,
    /// An action capable of deleting or irreversibly replacing local state.
    Destructive,
    /// A push, release, external message, or equivalent publication.
    Publish,
    /// Read or mutation of a resource outside the approved worktree root.
    OutsideRoot,
}

impl ActionClass {
    /// Validates a persisted risk projection without accepting a caller-selected lower risk.
    #[must_use]
    pub const fn permits_risk(self, risk: RiskLevel) -> bool {
        matches!(
            (self, risk),
            (Self::Read | Self::Derive, RiskLevel::Low)
                | (Self::Write | Self::ExecuteSafe, RiskLevel::Moderate)
                | (Self::ExecuteOpen, RiskLevel::High | RiskLevel::Critical)
                | (Self::Network, RiskLevel::High)
                | (
                    Self::Destructive | Self::Publish | Self::OutsideRoot,
                    RiskLevel::Critical
                )
        )
    }
}

/// Coarse risk used by approval presentation and durable audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RiskLevel {
    /// No mutation and no boundary crossing.
    Low,
    /// Bounded local mutation or a validated safe local process.
    Moderate,
    /// Arbitrary local execution or network communication.
    High,
    /// Destruction, publication, shell mode, or access outside approved roots.
    Critical,
}

/// Baseline or workspace-restricted disposition before a matching approval is consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PolicyDisposition {
    /// The fixed system baseline permits automatic execution.
    Automatic,
    /// An explicit, exact, live user approval is required.
    ApprovalRequired,
    /// The workspace owner tightened policy to disallow this action.
    Denied,
}

impl PolicyDisposition {
    const fn strictness(self) -> u8 {
        match self {
            Self::Automatic => 0,
            Self::ApprovalRequired => 1,
            Self::Denied => 2,
        }
    }
}

/// Operation over an approved worktree root as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RootPolicyOperation {
    /// Observe root-level metadata.
    Read,
    /// Build a deterministic local projection.
    Derive,
}

/// Exact or explicitly selected subtree coverage for a path action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathScopeCoverage {
    /// Only the named path is in scope.
    Exact,
    /// The named directory and descendants are in scope.
    Subtree,
}

/// Content-free path scope after the filesystem adapter has classified its root boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PolicyPathScope {
    /// A lossless repository-relative path under one exact worktree.
    Worktree {
        /// Owning approved worktree.
        worktree_id: WorktreeId,
        /// Canonical repository-relative path.
        path: RepositoryPath,
        /// Exact path or explicit subtree coverage.
        coverage: PathScopeCoverage,
    },
    /// An adapter-derived identity for a canonical resource outside approved roots.
    OutsideRoot {
        /// Digest identity; raw external paths do not enter durable policy state.
        resource_id: PolicyResourceId,
    },
}

/// Requested filesystem operation after root and path classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathPolicyOperation {
    /// Bounded read.
    Read,
    /// Add, update, or move content without implicit deletion.
    Write,
    /// Delete or replace existing content irreversibly.
    Delete,
}

/// Whether a process is a discovered safe command, arbitrary argv, or explicit shell mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessExecutionMode {
    /// Known test, build, lint, or formatter command executed directly as argv.
    KnownSafe,
    /// Arbitrary executable invoked directly as argv.
    Open,
    /// Explicit shell interpretation, always critical and approval-bound.
    Shell,
}

/// Proof that an automatically executable command belongs to the validated current plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessPlanBinding {
    /// No validated task-step binding was supplied.
    Unbound,
    /// Exact validated task step authorizing the known command category.
    Validated(TaskStepId),
}

/// Network boundary declared by a process specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessNetworkScope {
    /// Policy declares that the process must not use network access; V1 is not an OS sandbox.
    Denied,
    /// One explicit content-free network target is requested.
    Requested(PolicyResourceId),
}

/// Typed process policy input; the executable, argv, cwd, and environment are represented by a
/// digest produced only after the later ProcessSpec validator has bounded them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessPolicyAction {
    worktree_id: WorktreeId,
    specification_id: PolicyResourceId,
    mode: ProcessExecutionMode,
    plan_binding: ProcessPlanBinding,
    network: ProcessNetworkScope,
}

impl ProcessPolicyAction {
    /// Creates a process policy action from an already bounded ProcessSpec identity.
    #[must_use]
    pub const fn new(
        worktree_id: WorktreeId,
        specification_id: PolicyResourceId,
        mode: ProcessExecutionMode,
        plan_binding: ProcessPlanBinding,
        network: ProcessNetworkScope,
    ) -> Self {
        Self {
            worktree_id,
            specification_id,
            mode,
            plan_binding,
            network,
        }
    }

    /// Returns the owning worktree.
    #[must_use]
    pub const fn worktree_id(self) -> WorktreeId {
        self.worktree_id
    }

    /// Returns the content-free ProcessSpec identity.
    #[must_use]
    pub const fn specification_id(self) -> PolicyResourceId {
        self.specification_id
    }

    /// Returns the execution mode.
    #[must_use]
    pub const fn mode(self) -> ProcessExecutionMode {
        self.mode
    }

    /// Returns the validated-plan binding.
    #[must_use]
    pub const fn plan_binding(self) -> ProcessPlanBinding {
        self.plan_binding
    }

    /// Returns the declared network scope.
    #[must_use]
    pub const fn network(self) -> ProcessNetworkScope {
        self.network
    }
}

/// Purpose of an explicit network operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetworkPurpose {
    /// Local-model or explicitly configured remote model request.
    ModelProvider,
    /// Package or tool installation.
    PackageInstallation,
    /// Repository synchronization without publication.
    RepositorySync,
    /// Any other explicit remote API or download.
    ExternalService,
}

/// Git operation with a fixed security classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GitPolicyOperation {
    /// `git status`.
    Status,
    /// Bounded `git diff`.
    Diff,
    /// Bounded `git log`.
    Log,
    /// Bounded `git show`.
    Show,
    /// `git rev-parse` without mutation or network.
    RevParse,
    /// `git ls-files`.
    ListFiles,
    /// Create a local commit.
    Commit,
    /// Create a local branch.
    CreateBranch,
    /// Switch branches without an explicitly destructive overwrite.
    Checkout,
    /// Delete a local branch.
    DeleteBranch,
    /// Rewrite commits.
    Rebase,
    /// Merge histories.
    Merge,
    /// Reset local state.
    Reset,
    /// Remove untracked content.
    Clean,
    /// Checkout while discarding file changes.
    CheckoutWithLoss,
    /// Download remote state.
    Fetch,
    /// Download and integrate remote state.
    Pull,
    /// Publish local state to a remote.
    Push,
}

/// One typed request presented to the central policy engine.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PolicyAction {
    /// Root metadata observation or deterministic derivation.
    Root {
        /// Exact worktree root.
        worktree_id: WorktreeId,
        /// Read or derive operation.
        operation: RootPolicyOperation,
    },
    /// One path operation after adapter-side canonicalization and boundary classification.
    Path {
        /// Exact or outside-root target.
        scope: PolicyPathScope,
        /// Read, write, or destructive delete.
        operation: PathPolicyOperation,
    },
    /// One complete snapshot- and content-bound structured patch.
    Patch(PatchPolicyAction),
    /// One bounded direct process or explicit shell request.
    Process(ProcessPolicyAction),
    /// One explicit standalone network action.
    Network {
        /// Content-free exact target identity.
        target_id: PolicyResourceId,
        /// Closed purpose classification.
        purpose: NetworkPurpose,
    },
    /// One Git action within an exact worktree.
    Git {
        /// Exact worktree whose repository is affected.
        worktree_id: WorktreeId,
        /// Closed Git operation.
        operation: GitPolicyOperation,
    },
}

impl PolicyAction {
    /// Derives the only valid security class for this typed action.
    #[must_use]
    pub const fn class(&self) -> ActionClass {
        match self {
            Self::Root { operation, .. } => match operation {
                RootPolicyOperation::Read => ActionClass::Read,
                RootPolicyOperation::Derive => ActionClass::Derive,
            },
            Self::Path { scope, operation } => match scope {
                PolicyPathScope::OutsideRoot { .. } => ActionClass::OutsideRoot,
                PolicyPathScope::Worktree { .. } => match operation {
                    PathPolicyOperation::Read => ActionClass::Read,
                    PathPolicyOperation::Write => ActionClass::Write,
                    PathPolicyOperation::Delete => ActionClass::Destructive,
                },
            },
            Self::Patch(patch) => {
                if patch.destructive() {
                    ActionClass::Destructive
                } else {
                    ActionClass::Write
                }
            }
            Self::Process(process) => match process.network() {
                ProcessNetworkScope::Requested(_) => ActionClass::Network,
                ProcessNetworkScope::Denied => match process.mode() {
                    ProcessExecutionMode::KnownSafe => ActionClass::ExecuteSafe,
                    ProcessExecutionMode::Open | ProcessExecutionMode::Shell => {
                        ActionClass::ExecuteOpen
                    }
                },
            },
            Self::Network { .. } => ActionClass::Network,
            Self::Git { operation, .. } => match operation {
                GitPolicyOperation::Status
                | GitPolicyOperation::Diff
                | GitPolicyOperation::Log
                | GitPolicyOperation::Show
                | GitPolicyOperation::RevParse
                | GitPolicyOperation::ListFiles => ActionClass::Read,
                GitPolicyOperation::Commit
                | GitPolicyOperation::CreateBranch
                | GitPolicyOperation::Checkout => ActionClass::Write,
                GitPolicyOperation::DeleteBranch
                | GitPolicyOperation::Rebase
                | GitPolicyOperation::Merge
                | GitPolicyOperation::Reset
                | GitPolicyOperation::Clean
                | GitPolicyOperation::CheckoutWithLoss => ActionClass::Destructive,
                GitPolicyOperation::Fetch | GitPolicyOperation::Pull => ActionClass::Network,
                GitPolicyOperation::Push => ActionClass::Publish,
            },
        }
    }

    /// Derives the risk level; explicit shell mode is critical even though its class is ExecuteOpen.
    #[must_use]
    pub const fn risk(&self) -> RiskLevel {
        if matches!(
            self,
            Self::Process(ProcessPolicyAction {
                mode: ProcessExecutionMode::Shell,
                ..
            })
        ) {
            return RiskLevel::Critical;
        }
        match self.class() {
            ActionClass::Read | ActionClass::Derive => RiskLevel::Low,
            ActionClass::Write | ActionClass::ExecuteSafe => RiskLevel::Moderate,
            ActionClass::ExecuteOpen | ActionClass::Network => RiskLevel::High,
            ActionClass::Destructive | ActionClass::Publish | ActionClass::OutsideRoot => {
                RiskLevel::Critical
            }
        }
    }

    /// Derives the immutable action fingerprint used by an exact one-time approval.
    #[must_use]
    pub fn fingerprint(&self) -> PolicyActionFingerprint {
        PolicyActionFingerprint(derive_action_digest(POLICY_ACTION_DIGEST_DOMAIN, self))
    }

    /// Derives the content-free exact scope identity retained by audit and approval storage.
    #[must_use]
    pub fn scope_digest(&self) -> PolicyScopeDigest {
        PolicyScopeDigest(derive_action_digest(POLICY_SCOPE_DIGEST_DOMAIN, self))
    }
}

/// Exact digest of action semantics; debug output never reveals path or external resource data.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PolicyActionFingerprint([u8; 32]);

impl PolicyActionFingerprint {
    /// Reconstructs a persisted fingerprint.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical persisted representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for PolicyActionFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PolicyActionFingerprint([REDACTED])")
    }
}

/// Content-free digest of the exact approved scope.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PolicyScopeDigest([u8; 32]);

impl PolicyScopeDigest {
    /// Reconstructs a persisted scope digest.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the canonical persisted representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for PolicyScopeDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PolicyScopeDigest([REDACTED])")
    }
}

/// Immutable V1 system baseline from ADR-0012.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemPolicyV1;

impl SystemPolicyV1 {
    /// Returns the fixed baseline disposition. Safe process execution is automatic only for a
    /// validated plan binding and no network; all other privileged actions require approval.
    #[must_use]
    pub const fn disposition(self, action: &PolicyAction) -> PolicyDisposition {
        match action {
            PolicyAction::Root { .. } => PolicyDisposition::Automatic,
            PolicyAction::Path { scope, operation } => match (scope, operation) {
                (PolicyPathScope::Worktree { .. }, PathPolicyOperation::Read) => {
                    PolicyDisposition::Automatic
                }
                _ => PolicyDisposition::ApprovalRequired,
            },
            PolicyAction::Patch(_) => PolicyDisposition::ApprovalRequired,
            PolicyAction::Process(process)
                if matches!(process.mode(), ProcessExecutionMode::KnownSafe)
                    && matches!(process.plan_binding(), ProcessPlanBinding::Validated(_))
                    && matches!(process.network(), ProcessNetworkScope::Denied) =>
            {
                PolicyDisposition::Automatic
            }
            PolicyAction::Git {
                operation:
                    GitPolicyOperation::Status
                    | GitPolicyOperation::Diff
                    | GitPolicyOperation::Log
                    | GitPolicyOperation::Show
                    | GitPolicyOperation::RevParse
                    | GitPolicyOperation::ListFiles,
                ..
            } => PolicyDisposition::Automatic,
            PolicyAction::Process(_) | PolicyAction::Network { .. } | PolicyAction::Git { .. } => {
                PolicyDisposition::ApprovalRequired
            }
        }
    }
}

/// Workspace policy can only make one system action class stricter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspacePolicyRestriction {
    /// Upgrade automatic execution to explicit approval.
    RequireApproval,
    /// Deny the class even if the system baseline would otherwise allow it.
    Deny,
}

impl WorkspacePolicyRestriction {
    const fn disposition(self) -> PolicyDisposition {
        match self {
            Self::RequireApproval => PolicyDisposition::ApprovalRequired,
            Self::Deny => PolicyDisposition::Denied,
        }
    }
}

/// One class-level restrictive rule from a dedicated trusted workspace policy file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspacePolicyRule {
    class: ActionClass,
    restriction: WorkspacePolicyRestriction,
}

impl WorkspacePolicyRule {
    /// Creates a rule that cannot express an allow or lower a baseline requirement.
    #[must_use]
    pub const fn new(class: ActionClass, restriction: WorkspacePolicyRestriction) -> Self {
        Self { class, restriction }
    }

    /// Returns the affected action class.
    #[must_use]
    pub const fn class(self) -> ActionClass {
        self.class
    }

    /// Returns the stricter effect.
    #[must_use]
    pub const fn restriction(self) -> WorkspacePolicyRestriction {
        self.restriction
    }
}

/// Canonical bounded workspace overlay that has no representation for loosening system policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePolicy {
    rules: Vec<WorkspacePolicyRule>,
}

impl WorkspacePolicy {
    /// Empty overlay preserves the fixed system baseline.
    #[must_use]
    pub const fn unrestricted() -> Self {
        Self { rules: Vec::new() }
    }

    /// Canonicalizes unique class rules and rejects ambiguous duplicates or excessive input.
    pub fn new(mut rules: Vec<WorkspacePolicyRule>) -> Result<Self, WorkspacePolicyError> {
        if rules.len() > MAX_WORKSPACE_POLICY_RULES {
            return Err(WorkspacePolicyError::TooManyRules);
        }
        rules.sort_by_key(|rule| rule.class());
        if rules
            .windows(2)
            .any(|pair| pair[0].class() == pair[1].class())
        {
            return Err(WorkspacePolicyError::DuplicateClass);
        }
        Ok(Self { rules })
    }

    /// Applies only the stricter of the system baseline and matching workspace restriction.
    #[must_use]
    pub fn apply(&self, class: ActionClass, baseline: PolicyDisposition) -> PolicyDisposition {
        let workspace = self
            .rules
            .binary_search_by_key(&class, |rule| rule.class())
            .ok()
            .map(|index| self.rules[index].restriction().disposition());
        match workspace {
            Some(candidate) if candidate.strictness() > baseline.strictness() => candidate,
            Some(_) | None => baseline,
        }
    }

    /// Returns the canonical restrictive rules.
    #[must_use]
    pub fn rules(&self) -> &[WorkspacePolicyRule] {
        &self.rules
    }
}

/// Invalid trusted-workspace policy overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspacePolicyError {
    /// More than one rule targeted the same class.
    DuplicateClass,
    /// Rule input exceeded the number of closed action classes.
    TooManyRules,
}

impl fmt::Display for WorkspacePolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DuplicateClass => "workspace policy contains a duplicate action class",
            Self::TooManyRules => "workspace policy exceeds the action-class rule limit",
        })
    }
}

impl Error for WorkspacePolicyError {}

fn derive_action_digest(domain: &str, action: &PolicyAction) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(domain);
    match action {
        PolicyAction::Root {
            worktree_id,
            operation,
        } => {
            hash_tag(&mut hasher, 0);
            hasher.update(worktree_id.as_bytes());
            hash_tag(
                &mut hasher,
                match operation {
                    RootPolicyOperation::Read => 0,
                    RootPolicyOperation::Derive => 1,
                },
            );
        }
        PolicyAction::Path { scope, operation } => {
            hash_tag(&mut hasher, 1);
            match scope {
                PolicyPathScope::Worktree {
                    worktree_id,
                    path,
                    coverage,
                } => {
                    hash_tag(&mut hasher, 0);
                    hasher.update(worktree_id.as_bytes());
                    hash_bytes(&mut hasher, path.as_bytes());
                    hash_tag(
                        &mut hasher,
                        match coverage {
                            PathScopeCoverage::Exact => 0,
                            PathScopeCoverage::Subtree => 1,
                        },
                    );
                }
                PolicyPathScope::OutsideRoot { resource_id } => {
                    hash_tag(&mut hasher, 1);
                    hasher.update(resource_id.as_bytes());
                }
            }
            hash_tag(
                &mut hasher,
                match operation {
                    PathPolicyOperation::Read => 0,
                    PathPolicyOperation::Write => 1,
                    PathPolicyOperation::Delete => 2,
                },
            );
        }
        PolicyAction::Patch(patch) => {
            hash_tag(&mut hasher, 5);
            hasher.update(patch.worktree_id().as_bytes());
            if domain == POLICY_ACTION_DIGEST_DOMAIN {
                hasher.update(&patch.action_digest().as_bytes());
            } else {
                hasher.update(&patch.scope_digest().as_bytes());
            }
            hash_tag(&mut hasher, u8::from(patch.destructive()));
        }
        PolicyAction::Process(process) => {
            hash_tag(&mut hasher, 2);
            hasher.update(process.worktree_id().as_bytes());
            hasher.update(process.specification_id().as_bytes());
            hash_tag(
                &mut hasher,
                match process.mode() {
                    ProcessExecutionMode::KnownSafe => 0,
                    ProcessExecutionMode::Open => 1,
                    ProcessExecutionMode::Shell => 2,
                },
            );
            match process.plan_binding() {
                ProcessPlanBinding::Unbound => hash_tag(&mut hasher, 0),
                ProcessPlanBinding::Validated(step_id) => {
                    hash_tag(&mut hasher, 1);
                    hasher.update(step_id.as_bytes());
                }
            }
            match process.network() {
                ProcessNetworkScope::Denied => hash_tag(&mut hasher, 0),
                ProcessNetworkScope::Requested(target) => {
                    hash_tag(&mut hasher, 1);
                    hasher.update(target.as_bytes());
                }
            }
        }
        PolicyAction::Network { target_id, purpose } => {
            hash_tag(&mut hasher, 3);
            hasher.update(target_id.as_bytes());
            hash_tag(
                &mut hasher,
                match purpose {
                    NetworkPurpose::ModelProvider => 0,
                    NetworkPurpose::PackageInstallation => 1,
                    NetworkPurpose::RepositorySync => 2,
                    NetworkPurpose::ExternalService => 3,
                },
            );
        }
        PolicyAction::Git {
            worktree_id,
            operation,
        } => {
            hash_tag(&mut hasher, 4);
            hasher.update(worktree_id.as_bytes());
            hash_tag(&mut hasher, git_operation_tag(*operation));
        }
    }
    *hasher.finalize().as_bytes()
}

fn hash_tag(hasher: &mut blake3::Hasher, value: u8) {
    hasher.update(&[value]);
}

fn hash_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

const fn git_operation_tag(operation: GitPolicyOperation) -> u8 {
    match operation {
        GitPolicyOperation::Status => 0,
        GitPolicyOperation::Diff => 1,
        GitPolicyOperation::Log => 2,
        GitPolicyOperation::Show => 3,
        GitPolicyOperation::RevParse => 4,
        GitPolicyOperation::ListFiles => 5,
        GitPolicyOperation::Commit => 6,
        GitPolicyOperation::CreateBranch => 7,
        GitPolicyOperation::Checkout => 8,
        GitPolicyOperation::DeleteBranch => 9,
        GitPolicyOperation::Rebase => 10,
        GitPolicyOperation::Merge => 11,
        GitPolicyOperation::Reset => 12,
        GitPolicyOperation::Clean => 13,
        GitPolicyOperation::CheckoutWithLoss => 14,
        GitPolicyOperation::Fetch => 15,
        GitPolicyOperation::Pull => 16,
        GitPolicyOperation::Push => 17,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActionClass, GitPolicyOperation, PathPolicyOperation, PathScopeCoverage, PolicyAction,
        PolicyDisposition, PolicyPathScope, ProcessExecutionMode, ProcessNetworkScope,
        ProcessPlanBinding, ProcessPolicyAction, RiskLevel, SystemPolicyV1, WorkspacePolicy,
        WorkspacePolicyError, WorkspacePolicyRestriction, WorkspacePolicyRule,
    };
    use crate::{PolicyResourceId, RepositoryPath, TaskStepId, WorktreeId};

    #[test]
    fn action_class_and_risk_are_derived_from_closed_operations()
    -> Result<(), Box<dyn std::error::Error>> {
        let worktree = WorktreeId::from_bytes([1; 32]);
        let cases = [
            (
                PolicyAction::Git {
                    worktree_id: worktree,
                    operation: GitPolicyOperation::Status,
                },
                ActionClass::Read,
                RiskLevel::Low,
            ),
            (
                PolicyAction::Git {
                    worktree_id: worktree,
                    operation: GitPolicyOperation::Reset,
                },
                ActionClass::Destructive,
                RiskLevel::Critical,
            ),
            (
                PolicyAction::Git {
                    worktree_id: worktree,
                    operation: GitPolicyOperation::Push,
                },
                ActionClass::Publish,
                RiskLevel::Critical,
            ),
            (
                PolicyAction::Path {
                    scope: PolicyPathScope::OutsideRoot {
                        resource_id: PolicyResourceId::from_bytes([2; 32]),
                    },
                    operation: PathPolicyOperation::Read,
                },
                ActionClass::OutsideRoot,
                RiskLevel::Critical,
            ),
            (
                PolicyAction::Process(ProcessPolicyAction::new(
                    worktree,
                    PolicyResourceId::from_bytes([3; 32]),
                    ProcessExecutionMode::Shell,
                    ProcessPlanBinding::Unbound,
                    ProcessNetworkScope::Denied,
                )),
                ActionClass::ExecuteOpen,
                RiskLevel::Critical,
            ),
        ];
        for (action, class, risk) in cases {
            assert_eq!(action.class(), class);
            assert_eq!(action.risk(), risk);
        }

        let first = PolicyAction::Path {
            scope: PolicyPathScope::Worktree {
                worktree_id: worktree,
                path: RepositoryPath::try_from_bytes(b"src/first.rs".to_vec())?,
                coverage: PathScopeCoverage::Exact,
            },
            operation: PathPolicyOperation::Write,
        };
        let second = PolicyAction::Path {
            scope: PolicyPathScope::Worktree {
                worktree_id: worktree,
                path: RepositoryPath::try_from_bytes(b"src/second.rs".to_vec())?,
                coverage: PathScopeCoverage::Exact,
            },
            operation: PathPolicyOperation::Write,
        };
        assert_ne!(first.fingerprint(), second.fingerprint());
        assert_ne!(first.scope_digest(), second.scope_digest());
        Ok(())
    }

    #[test]
    fn only_plan_bound_network_free_known_process_is_automatic() {
        let worktree = WorktreeId::from_bytes([1; 32]);
        let spec = PolicyResourceId::from_bytes([2; 32]);
        let policy = SystemPolicyV1;
        let safe = PolicyAction::Process(ProcessPolicyAction::new(
            worktree,
            spec,
            ProcessExecutionMode::KnownSafe,
            ProcessPlanBinding::Validated(TaskStepId::from_bytes([3; 32])),
            ProcessNetworkScope::Denied,
        ));
        let unbound = PolicyAction::Process(ProcessPolicyAction::new(
            worktree,
            spec,
            ProcessExecutionMode::KnownSafe,
            ProcessPlanBinding::Unbound,
            ProcessNetworkScope::Denied,
        ));
        let networked = PolicyAction::Process(ProcessPolicyAction::new(
            worktree,
            spec,
            ProcessExecutionMode::KnownSafe,
            ProcessPlanBinding::Validated(TaskStepId::from_bytes([3; 32])),
            ProcessNetworkScope::Requested(PolicyResourceId::from_bytes([4; 32])),
        ));

        assert_eq!(policy.disposition(&safe), PolicyDisposition::Automatic);
        assert_eq!(
            policy.disposition(&unbound),
            PolicyDisposition::ApprovalRequired
        );
        assert_eq!(networked.class(), ActionClass::Network);
        assert_eq!(
            policy.disposition(&networked),
            PolicyDisposition::ApprovalRequired
        );
    }

    #[test]
    fn workspace_overlay_has_no_representation_that_can_loosen_system_policy()
    -> Result<(), Box<dyn std::error::Error>> {
        let overlay = WorkspacePolicy::new(vec![
            WorkspacePolicyRule::new(
                ActionClass::Read,
                WorkspacePolicyRestriction::RequireApproval,
            ),
            WorkspacePolicyRule::new(ActionClass::Publish, WorkspacePolicyRestriction::Deny),
        ])?;
        assert_eq!(
            overlay.apply(ActionClass::Read, PolicyDisposition::Automatic),
            PolicyDisposition::ApprovalRequired
        );
        assert_eq!(
            overlay.apply(ActionClass::Publish, PolicyDisposition::ApprovalRequired),
            PolicyDisposition::Denied
        );
        assert_eq!(
            WorkspacePolicy::unrestricted()
                .apply(ActionClass::Network, PolicyDisposition::ApprovalRequired),
            PolicyDisposition::ApprovalRequired
        );
        assert_eq!(
            WorkspacePolicy::new(vec![
                WorkspacePolicyRule::new(ActionClass::Read, WorkspacePolicyRestriction::Deny,),
                WorkspacePolicyRule::new(
                    ActionClass::Read,
                    WorkspacePolicyRestriction::RequireApproval,
                ),
            ]),
            Err(WorkspacePolicyError::DuplicateClass)
        );
        Ok(())
    }
}
