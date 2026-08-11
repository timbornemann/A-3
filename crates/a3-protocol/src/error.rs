use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Stable error codes exposed by V1 IPC commands.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCodeV1 {
    /// The request used a protocol version this build does not support.
    UnsupportedProtocolVersion,
    /// The native picker did not return a usable local directory.
    ProjectSelectionFailed,
    /// The selected directory disappeared or became inaccessible before validation.
    ProjectSelectionUnavailable,
    /// The selected directory is not a Git repository root.
    NotGitRepository,
    /// The selection was nested inside a different Git worktree root.
    ProjectRootRequired,
    /// The selected Git repository shape is intentionally unsupported.
    UnsupportedRepository,
    /// Required local Git metadata was malformed or inconsistent.
    InvalidRepositoryMetadata,
    /// The private local database could not be reached or written.
    LocalStorageUnavailable,
    /// The private local database failed integrity checks.
    LocalStorageCorrupt,
    /// The private local database was created by a newer A^3 build.
    LocalStorageUpgradeRequired,
    /// The private local database violated its versioned logical schema.
    LocalStorageInvalidData,
    /// Stored and newly observed project identities conflict.
    ProjectIdentityConflict,
    /// No Core-owned active project exists for the requested operation.
    NoActiveProject,
    /// A repository-tree path token, cursor, or limit violated the strict query contract.
    InvalidRepositoryTreeQuery,
    /// A previously visible indexed directory is absent from the current publication.
    RepositoryTreeDirectoryUnavailable,
    /// The active project already has a queued or running rebuild.
    IndexRebuildAlreadyPending,
    /// The owned index coordinator could not accept a rebuild request.
    IndexRebuildUnavailable,
    /// Another Core-owned project lifecycle operation is still in progress.
    ProjectOperationBusy,
    /// The exact active worktree was no longer present in the project list.
    ProjectNotInList,
    /// The active project could not be safely removed from the project list.
    ProjectRemovalUnavailable,
    /// No live-verified mapping model and complete Deep-Map executor are configured.
    DeepMapUnavailable,
    /// The supplied token, wall-time, or read-only-tool budget was outside fixed bounds.
    InvalidDeepMapBudget,
    /// A start or resume was requested while another Deep-Map action was pending.
    DeepMapAlreadyPending,
    /// Pause or cancel was requested without a running Deep-Map attempt.
    DeepMapNotRunning,
    /// Resume was requested without a validated paused checkpoint.
    DeepMapNotPaused,
}

/// Safe, versioned error returned across the IPC boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CommandErrorV1 {
    protocol_version: ProtocolVersion,
    code: ErrorCodeV1,
    message: String,
}

impl CommandErrorV1 {
    /// Creates the stable response for an unsupported request version.
    #[must_use]
    pub fn unsupported_protocol_version() -> Self {
        Self::new(
            ErrorCodeV1::UnsupportedProtocolVersion,
            "The requested protocol version is not supported.",
        )
    }

    /// Creates a safe project-open failure from an already classified boundary code.
    #[must_use]
    pub fn project_open(code: ErrorCodeV1) -> Self {
        let message = match code {
            ErrorCodeV1::ProjectSelectionFailed => {
                "The native directory selection could not be used."
            }
            ErrorCodeV1::ProjectSelectionUnavailable => {
                "The selected directory is no longer available."
            }
            ErrorCodeV1::NotGitRepository => "Select the root directory of a local Git repository.",
            ErrorCodeV1::ProjectRootRequired => {
                "Select the Git worktree root rather than one of its subdirectories."
            }
            ErrorCodeV1::UnsupportedRepository => "This Git repository layout is not supported.",
            ErrorCodeV1::InvalidRepositoryMetadata => {
                "The selected repository metadata could not be validated."
            }
            ErrorCodeV1::LocalStorageUnavailable => "Local A^3 storage is unavailable.",
            ErrorCodeV1::LocalStorageCorrupt => {
                "Local A^3 storage is damaged and was not modified."
            }
            ErrorCodeV1::LocalStorageUpgradeRequired => {
                "Local A^3 storage was created by a newer application version."
            }
            ErrorCodeV1::LocalStorageInvalidData => {
                "Local A^3 storage contains invalid project data."
            }
            ErrorCodeV1::ProjectIdentityConflict => {
                "The selected worktree conflicts with its stored project identity."
            }
            ErrorCodeV1::NoActiveProject => "Open a local Git worktree before using this action.",
            ErrorCodeV1::InvalidRepositoryTreeQuery => {
                "The repository tree request is outside the supported bounds."
            }
            ErrorCodeV1::RepositoryTreeDirectoryUnavailable => {
                "The selected directory is no longer present in the published index."
            }
            ErrorCodeV1::IndexRebuildAlreadyPending => {
                "An index rebuild is already queued or running for the active worktree."
            }
            ErrorCodeV1::IndexRebuildUnavailable => {
                "The local index coordinator could not accept the rebuild request."
            }
            ErrorCodeV1::ProjectOperationBusy => "Another project operation is still in progress.",
            ErrorCodeV1::ProjectNotInList => {
                "The active worktree is no longer in the A^3 project list."
            }
            ErrorCodeV1::ProjectRemovalUnavailable => {
                "The active worktree could not be safely removed from the A^3 project list."
            }
            ErrorCodeV1::DeepMapUnavailable => {
                "Deep Map requires a live-verified local mapping model."
            }
            ErrorCodeV1::InvalidDeepMapBudget => {
                "The selected Deep Map budget is outside the supported limits."
            }
            ErrorCodeV1::DeepMapAlreadyPending => {
                "A Deep Map action or paused checkpoint is already active for the worktree."
            }
            ErrorCodeV1::DeepMapNotRunning => "No Deep Map attempt is currently running.",
            ErrorCodeV1::DeepMapNotPaused => {
                "No validated paused Deep Map checkpoint is available."
            }
            ErrorCodeV1::UnsupportedProtocolVersion => {
                "The requested protocol version is not supported."
            }
        };
        Self::new(code, message)
    }

    /// Creates a safe active-project rebuild failure.
    #[must_use]
    pub fn project_rebuild(code: ErrorCodeV1) -> Self {
        Self::project_open(code)
    }

    /// Creates a safe active-project removal failure.
    #[must_use]
    pub fn project_removal(code: ErrorCodeV1) -> Self {
        Self::project_open(code)
    }

    /// Creates a safe Deep-Map lifecycle failure.
    #[must_use]
    pub fn deep_map(code: ErrorCodeV1) -> Self {
        Self::project_open(code)
    }

    fn new(code: ErrorCodeV1, message: &str) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            code,
            message: message.to_owned(),
        }
    }

    /// Returns the protocol version used to encode this error.
    #[must_use]
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Returns the stable, localizable error code.
    #[must_use]
    pub const fn code(&self) -> ErrorCodeV1 {
        self.code
    }

    /// Returns the safe error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandErrorV1, ErrorCodeV1};
    use crate::ProtocolVersion;
    use serde_json::json;

    #[test]
    fn unsupported_version_error_has_stable_safe_shape() -> Result<(), serde_json::Error> {
        let error = CommandErrorV1::unsupported_protocol_version();

        assert_eq!(error.protocol_version(), ProtocolVersion::V1);
        assert_eq!(error.code(), ErrorCodeV1::UnsupportedProtocolVersion);
        assert_eq!(
            serde_json::to_value(&error)?,
            json!({
                "code": "unsupportedProtocolVersion",
                "message": "The requested protocol version is not supported.",
                "protocolVersion": 1
            })
        );
        Ok(())
    }

    #[test]
    fn project_errors_never_contain_adapter_details() {
        let error = CommandErrorV1::project_open(ErrorCodeV1::NotGitRepository);

        assert_eq!(error.code(), ErrorCodeV1::NotGitRepository);
        assert_eq!(
            error.message(),
            "Select the root directory of a local Git repository."
        );
    }
}
