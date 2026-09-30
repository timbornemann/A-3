use a3_domain::{
    CanonicalDirectory, ContentHash, MachineFileAction, MachineFileOperation, PathPolicyOperation,
    PolicyAction, PolicyDecision, PolicyDecisionId, PolicyDecisionOutcome, PolicyDecisionReason,
    PolicyDisposition, PolicyResourceId, ProjectIdentity, RepositoryPath,
};
use std::{error::Error, fmt, future::Future, pin::Pin};

/// Bounded future of a canonical machine-file adapter.
pub type MachineFileToolFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, MachineFileToolFailure>> + Send + 'a>>;

/// Privileged preparation result. Canonicalization grants no execution authority.
#[derive(Debug, Clone)]
pub struct PreparedMachineFileAction {
    action: MachineFileAction,
    root: CanonicalDirectory,
    file: RepositoryPath,
    policy: PolicyAction,
}
impl PreparedMachineFileAction {
    /// Binds one adapter-proven canonical external parent and exact final filename.
    pub fn new(
        project: &ProjectIdentity,
        action: MachineFileAction,
        root: CanonicalDirectory,
        file: RepositoryPath,
        resource: PolicyResourceId,
    ) -> Result<Self, MachineFileToolFailure> {
        if file.as_bytes().contains(&b'/')
            || root
                .as_path()
                .starts_with(project.worktree().root().as_path())
        {
            return Err(MachineFileToolFailure::Denied);
        }
        let (operation, expected, proposed) = match action.operation() {
            MachineFileOperation::Read(hash) => (PathPolicyOperation::Read, *hash, None),
            MachineFileOperation::Write { expected, content } => (
                PathPolicyOperation::Write,
                *expected,
                Some(content.content_hash()),
            ),
            MachineFileOperation::Delete(hash) => (PathPolicyOperation::Delete, Some(*hash), None),
        };
        let policy = PolicyAction::MachineFile {
            worktree_id: project.worktree().id(),
            step_id: action.step_id(),
            resource_id: resource,
            operation,
            expected,
            proposed,
        };
        Ok(Self {
            action,
            root,
            file,
            policy,
        })
    }
    /// Exact content- and step-bound typed policy request.
    #[must_use]
    pub const fn policy_action(&self) -> &PolicyAction {
        &self.policy
    }
    /// Original full-file proposal.
    #[must_use]
    pub const fn action(&self) -> &MachineFileAction {
        &self.action
    }
    /// Adapter-proven external parent.
    #[must_use]
    pub const fn root(&self) -> &CanonicalDirectory {
        &self.root
    }
    /// Exact single component within that parent.
    #[must_use]
    pub const fn file(&self) -> &RepositoryPath {
        &self.file
    }
}

/// Single action capability minted only by an exact central policy decision.
#[derive(Debug)]
pub struct AuthorizedMachineFileAction {
    prepared: PreparedMachineFileAction,
    decision: PolicyDecision,
}
impl AuthorizedMachineFileAction {
    /// Rejects mismatched paths, hashes, steps, denied actions and fabricated automatic grants.
    pub fn new(
        prepared: PreparedMachineFileAction,
        decision: &PolicyDecision,
    ) -> Result<Self, MachineFileToolFailure> {
        let action = prepared.policy_action();
        let allowed = decision.reason() == PolicyDecisionReason::ApprovalGranted
            || (decision.reason() == PolicyDecisionReason::SystemAutomatic
                && decision.permission_settings().is_some_and(|settings| {
                    settings.disposition(action) == PolicyDisposition::Automatic
                }));
        if !allowed
            || decision.outcome() != PolicyDecisionOutcome::Allowed
            || decision.action_fingerprint() != action.fingerprint()
            || decision.scope_digest() != action.scope_digest()
            || decision.action_class() != action.class()
            || decision.risk_level() != action.risk()
        {
            return Err(MachineFileToolFailure::Denied);
        }
        Ok(Self {
            prepared,
            decision: decision.clone(),
        })
    }
    /// Exact typed proposal accepted by policy.
    #[must_use]
    pub const fn prepared(&self) -> &PreparedMachineFileAction {
        &self.prepared
    }
    /// Current authorization including its durable settings revision.
    #[must_use]
    pub const fn decision(&self) -> &PolicyDecision {
        &self.decision
    }
}

/// Bounded receipt of an actual file effect, never a model-authored completion claim.
#[derive(Debug, Clone)]
pub struct MachineFileReceipt {
    /// Exact canonical resource identity.
    pub resource: PolicyResourceId,
    /// Policy decision which actually authorized this effect.
    pub decision: PolicyDecisionId,
    /// Observed hash after a read or write; None means verified absence after deletion.
    pub hash: Option<ContentHash>,
    /// True only for a verified changed file state.
    pub changed: bool,
    /// Bounded secret-checked bytes for a read, never persisted as audit content.
    pub text: Option<a3_domain::PatchFileContent>,
}

/// Cooperative control for bounded filesystem effects.
pub trait MachineFileControl: fmt::Debug + Send + Sync {
    /// Stops admission and bounded reads before a filesystem mutation starts.
    fn is_cancelled(&self) -> bool;
}

/// Closed V1 complete-file adapter; no directory deletion or generic filesystem API.
pub trait MachineFileTool: fmt::Debug + Send + Sync {
    /// Canonicalizes metadata only; never reads source before exact authorization.
    fn prepare<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        action: &'a MachineFileAction,
        control: &'a dyn MachineFileControl,
    ) -> MachineFileToolFuture<'a, PreparedMachineFileAction>;
    /// Revalidates permission revision, canonical path, hash and secret boundaries before effects.
    fn execute<'a>(
        &'a self,
        authorized: AuthorizedMachineFileAction,
        control: &'a dyn MachineFileControl,
    ) -> MachineFileToolFuture<'a, MachineFileReceipt>;
}

/// Content-free machine-file failure; uncertain effects always require reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineFileToolFailure {
    /// Canonical, policy, type, secret or binary boundary denied the request.
    Denied,
    /// The expected absence, hash or canonical target changed.
    Stale,
    /// Permissions changed before admission; retry only through fresh central policy.
    PermissionsChanged,
    /// Cooperative cancellation prevented the next effect.
    Cancelled,
    /// Bounded storage operation failed before a known effect.
    Unavailable,
    /// A write may have happened; automatic retry is prohibited.
    ReconciliationRequired,
}
impl fmt::Display for MachineFileToolFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Denied => "machine file action denied",
            Self::Stale => "machine file state changed",
            Self::PermissionsChanged => "machine permissions changed before admission",
            Self::Cancelled => "machine file action cancelled",
            Self::Unavailable => "machine file action unavailable",
            Self::ReconciliationRequired => "machine file state needs reconciliation",
        })
    }
}
impl Error for MachineFileToolFailure {}
