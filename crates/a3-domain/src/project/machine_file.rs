use crate::{ContentHash, PatchFileContent, SecretCandidateClassifierV1, TaskStepId};
use std::{error::Error, fmt};

/// Bounded untrusted absolute path proposal. Only an adapter may canonicalize it.
#[derive(Clone, PartialEq, Eq)]
pub struct MachineFilePath(String);

impl MachineFilePath {
    /// Accepts a path proposal without granting filesystem authority.
    pub fn new(value: String) -> Result<Self, MachineFileActionError> {
        let path = value.strip_prefix("\\\\?\\").unwrap_or(&value);
        let absolute = path.starts_with('/')
            || (path.as_bytes().get(1) == Some(&b':')
                && path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                && matches!(path.as_bytes().get(2), Some(b'/' | b'\\')));
        if !absolute || value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(MachineFileActionError);
        }
        Ok(Self(value))
    }
    /// Exact proposal for privileged adapter validation; never a WebView capability.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for MachineFilePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MachineFilePath")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Version-one complete-file operations outside the current worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineFileOperation {
    /// Bounded read with an optional previously observed hash.
    Read(Option<ContentHash>),
    /// Create when absent, or replace exactly the expected current hash.
    Write {
        /// None admits creation only, never blind overwrite.
        expected: Option<ContentHash>,
        /// Complete bounded secret-checked UTF-8 content.
        content: PatchFileContent,
    },
    /// Delete one regular file with its exact observed hash; directories are unsupported.
    Delete(ContentHash),
}

/// Model proposal, with no canonical path, policy decision or execution authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineFileAction {
    step: TaskStepId,
    path: MachineFilePath,
    operation: MachineFileOperation,
}
impl MachineFileAction {
    /// Bounds one V1 proposal before it can reach privileged preparation.
    pub fn new(
        step: TaskStepId,
        path: MachineFilePath,
        operation: MachineFileOperation,
    ) -> Result<Self, MachineFileActionError> {
        if let MachineFileOperation::Write { content, .. } = &operation {
            let text =
                std::str::from_utf8(content.as_bytes()).map_err(|_| MachineFileActionError)?;
            if content.as_bytes().len() > 64 * 1024
                || SecretCandidateClassifierV1::classify(text).is_some()
            {
                return Err(MachineFileActionError);
            }
        }
        Ok(Self {
            step,
            path,
            operation,
        })
    }
    /// Owning active ledger step.
    #[must_use]
    pub const fn step_id(&self) -> TaskStepId {
        self.step
    }
    /// Untrusted absolute path proposal.
    #[must_use]
    pub const fn path(&self) -> &MachineFilePath {
        &self.path
    }
    /// Complete hash-bound operation.
    #[must_use]
    pub const fn operation(&self) -> &MachineFileOperation {
        &self.operation
    }
}

/// Invalid or oversized machine file proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineFileActionError;
impl fmt::Display for MachineFileActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid machine file proposal")
    }
}
impl Error for MachineFileActionError {}
