use crate::{PolicyAction, PolicyResourceId, SecretCandidateClassifierV1, TaskStepId, WorktreeId};
use std::{error::Error, fmt};

/// Version of the closed, credential-free machine HTTP read proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineHttpSchemaVersion {
    /// Bounded GET with no credentials, user headers, body or redirects.
    V1,
}

/// Bounded untrusted URL; the privileged adapter validates URL structure and transport.
#[derive(Clone, PartialEq, Eq)]
pub struct MachineHttpUrl(String);
impl MachineHttpUrl {
    /// Admits a URL proposal without granting network authority.
    pub fn new(value: String) -> Result<Self, MachineHttpError> {
        if value.is_empty()
            || value.len() > 4096
            || value.chars().any(|c| c.is_control() || c.is_whitespace())
            || SecretCandidateClassifierV1::classify(&value).is_some()
        {
            return Err(MachineHttpError);
        }
        Ok(Self(value))
    }
    /// Exact proposed URL for privileged parsing and user approval display.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for MachineHttpUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MachineHttpUrl")
            .field("bytes", &self.0.len())
            .finish()
    }
}

/// Plan-bound V1 HTTP read. Neither risk classification nor permission is model-owned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineHttpAction {
    step: TaskStepId,
    url: MachineHttpUrl,
}
impl MachineHttpAction {
    /// Binds a bounded proposal to a Core-validated current step.
    #[must_use]
    pub const fn new(step: TaskStepId, url: MachineHttpUrl) -> Self {
        Self { step, url }
    }
    /// Exact owning step.
    #[must_use]
    pub const fn step_id(&self) -> TaskStepId {
        self.step
    }
    /// Untrusted target proposal.
    #[must_use]
    pub const fn url(&self) -> &MachineHttpUrl {
        &self.url
    }
    /// Versioned closed transport contract.
    #[must_use]
    pub const fn version(&self) -> MachineHttpSchemaVersion {
        MachineHttpSchemaVersion::V1
    }
}

/// Core-owned canonical target of the single closed GET operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedMachineHttpAction {
    step: TaskStepId,
    url: MachineHttpUrl,
    worktree: WorktreeId,
    resource: PolicyResourceId,
}
impl PreparedMachineHttpAction {
    /// Called only after the privileged URL parser validates and canonicalizes the target.
    #[must_use]
    pub fn new(step: TaskStepId, url: MachineHttpUrl, worktree: WorktreeId) -> Self {
        let mut hash = blake3::Hasher::new_derive_key("a3.machine-http-get.v1");
        hash.update(url.as_str().as_bytes());
        Self {
            step,
            url,
            worktree,
            resource: PolicyResourceId::from_bytes(*hash.finalize().as_bytes()),
        }
    }
    /// Canonical target for one exact GET, without user-defined headers or body.
    #[must_use]
    pub const fn url(&self) -> &MachineHttpUrl {
        &self.url
    }
    /// Content-free target identity.
    #[must_use]
    pub const fn resource(&self) -> PolicyResourceId {
        self.resource
    }
    /// Exact GET semantics included in central policy.
    #[must_use]
    pub const fn policy_action(&self) -> PolicyAction {
        PolicyAction::MachineHttpGet {
            worktree_id: self.worktree,
            step_id: self.step,
            target_id: self.resource,
        }
    }
}

/// Content-free URL boundary failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineHttpError;
impl fmt::Display for MachineHttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid machine HTTP proposal")
    }
}
impl Error for MachineHttpError {}
