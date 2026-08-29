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
    /// A parent module ID, cursor, or limit violated the strict module-tree contract.
    InvalidModuleTreeQuery,
    /// A previously visible primary module is absent from the current projection.
    ModuleTreeParentUnavailable,
    /// A module dependency center ID or node limit violated the strict graph contract.
    InvalidModuleDependencyGraphQuery,
    /// A module ID or role-prefix limit violated the strict runtime-map contract.
    InvalidModuleRuntimeMapQuery,
    /// A publication, module, root, preset, or result limit violated the runtime-flow contract.
    InvalidModuleRuntimeFlowQuery,
    /// A module ID violated the strict Module Card detail contract.
    InvalidModuleCardDetailQuery,
    /// A Card, publication, module, or Evidence anchor violated the Inspector contract.
    InvalidModuleCardEvidenceQuery,
    /// A source-preview request contained anything except one valid Core-issued Evidence selection.
    InvalidProjectMapSourcePreviewQuery,
    /// Current Evidence source could not be safely revalidated for bounded display.
    ProjectMapSourcePreviewUnavailable,
    /// Search text was empty, oversized, or lacked a searchable token.
    InvalidProjectMapSearchQuery,
    /// A scene focus identity violated the strict atlas request contract.
    InvalidProjectMapSceneQuery,
    /// A task or active-plan step identity violated the strict Task Lens selector contract.
    InvalidTaskLensSelection,
    /// Durable anchors or deterministic Task Lens retrieval could not be read safely.
    TaskLensUnavailable,
    /// A task, volatile record, revision, stream, cursor, or page limit violated U6 bounds.
    InvalidAgentInspectionQuery,
    /// Exact diff, log, or durable verification state could not be read safely.
    AgentInspectionUnavailable,
    /// Goal Contract content, identities, or revision metadata violated the strict Agent contract.
    InvalidAgentGoal,
    /// The selected durable task is absent from the active worktree.
    AgentGoalTaskNotFound,
    /// Another writer advanced the Goal Contract after the editor loaded it.
    AgentGoalRevisionConflict,
    /// Goal Contract metadata or local persistence could not complete the requested operation.
    AgentGoalUnavailable,
    /// A task-bound Agent recovery request used invalid optimistic anchors.
    InvalidAgentTaskControl,
    /// Agent recovery state or its atomic control transaction could not be completed safely.
    AgentTaskControlUnavailable,
    /// Approval selectors or optimistic anchors violated the strict task-bound contract.
    InvalidAgentApprovalRequest,
    /// Exact approval state or its atomic control could not be completed safely.
    AgentApprovalUnavailable,
    /// Session identity, cursor, content, mode, or control violated the Agent chat contract.
    InvalidAgentSessionRequest,
    /// Another writer advanced the selected Agent session after it was displayed.
    AgentSessionRevisionConflict,
    /// Agent conversation persistence or the configured Coding model was unavailable.
    AgentSessionUnavailable,
    /// The selected Agent session already owns an incompatible active operation.
    AgentSessionBusy,
    /// The active project already has a queued or running rebuild.
    IndexRebuildAlreadyPending,
    /// The owned index coordinator could not accept a rebuild request.
    IndexRebuildUnavailable,
    /// Another Core-owned project lifecycle operation is still in progress.
    ProjectOperationBusy,
    /// Project-catalog search, cursor, direction, or worktree identity was invalid.
    InvalidProjectCatalogRequest,
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
    /// Settings request fields or optimistic revision violated the strict contract.
    InvalidSettingsRequest,
    /// The configured endpoint is invalid, unsafe, or remote-blocked for probing.
    ModelEndpointInvalid,
    /// Another explicit model discovery or capability operation is still active.
    ModelProbeAlreadyActive,
    /// Model settings or explicit probe could not be completed safely.
    ModelSettingsUnavailable,
    /// The supplied provider credential violated its strict secret envelope.
    ProviderCredentialInvalid,
    /// The active provider requires a credential before network operations are available.
    ProviderCredentialMissing,
    /// A partial credential transition must be repaired by replacing or deleting the key.
    ProviderCredentialRecoveryRequired,
    /// The native operating-system credential service is unavailable or locked.
    ProviderCredentialStoreUnavailable,
    /// Project Settings input was malformed, stale, or outside fixed bounds.
    InvalidProjectSettingsRequest,
    /// Dedicated project configuration, index, or allowlist storage was unavailable.
    ProjectSettingsUnavailable,
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
    /// Creates a safe Agent-session failure from an already classified boundary code.
    #[must_use]
    pub fn agent_session(code: ErrorCodeV1) -> Self {
        let message = match code {
            ErrorCodeV1::InvalidAgentSessionRequest => {
                "The Agent conversation request is outside the supported bounds."
            }
            ErrorCodeV1::AgentSessionRevisionConflict => {
                "The Agent conversation changed. Reload it before continuing."
            }
            ErrorCodeV1::AgentSessionBusy => {
                "This Agent conversation is already processing another action."
            }
            ErrorCodeV1::NoActiveProject => {
                "Open a local Git worktree before using the Agent workspace."
            }
            _ => "The Agent conversation could not be completed safely.",
        };
        Self::new(code, message)
    }

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
            ErrorCodeV1::InvalidModuleTreeQuery => {
                "The module tree request is outside the supported bounds."
            }
            ErrorCodeV1::ModuleTreeParentUnavailable => {
                "The selected module is no longer present in the published index."
            }
            ErrorCodeV1::InvalidModuleDependencyGraphQuery => {
                "The module dependency request is outside the supported bounds."
            }
            ErrorCodeV1::InvalidModuleRuntimeMapQuery => {
                "The module runtime-map request is outside the supported bounds."
            }
            ErrorCodeV1::InvalidModuleRuntimeFlowQuery => {
                "The module runtime-flow request is outside the supported bounds."
            }
            ErrorCodeV1::InvalidModuleCardDetailQuery => {
                "The Module Card detail request is outside the supported bounds."
            }
            ErrorCodeV1::InvalidModuleCardEvidenceQuery => {
                "The Module Card Evidence request is outside the supported bounds."
            }
            ErrorCodeV1::InvalidProjectMapSourcePreviewQuery => {
                "The source-preview request is outside the supported Evidence bounds."
            }
            ErrorCodeV1::ProjectMapSourcePreviewUnavailable => {
                "The selected source preview could not be read safely."
            }
            ErrorCodeV1::InvalidProjectMapSearchQuery => {
                "The Project Map search query is outside the supported bounds."
            }
            ErrorCodeV1::InvalidProjectMapSceneQuery => {
                "The Project Map scene request is outside the supported bounds."
            }
            ErrorCodeV1::InvalidTaskLensSelection => {
                "The Task Lens task or step selection is outside the supported bounds."
            }
            ErrorCodeV1::TaskLensUnavailable => {
                "The current Task Lens could not be compiled from local evidence."
            }
            ErrorCodeV1::InvalidAgentInspectionQuery => {
                "The Agent inspection request is outside the supported bounds."
            }
            ErrorCodeV1::AgentInspectionUnavailable => {
                "The Agent diff and verification inspection could not be read safely."
            }
            ErrorCodeV1::InvalidAgentGoal => {
                "The Goal Contract content is outside the supported bounds."
            }
            ErrorCodeV1::AgentGoalTaskNotFound => {
                "The selected Goal Contract is no longer available in this worktree."
            }
            ErrorCodeV1::AgentGoalRevisionConflict => {
                "The Goal Contract changed after this editor was opened. Reload it before revising."
            }
            ErrorCodeV1::AgentGoalUnavailable => {
                "The Goal Contract could not be read or stored safely."
            }
            ErrorCodeV1::InvalidAgentTaskControl => {
                "The Agent run control request is outside the supported bounds."
            }
            ErrorCodeV1::AgentTaskControlUnavailable => {
                "The Agent run could not be inspected or controlled safely."
            }
            ErrorCodeV1::InvalidAgentApprovalRequest => {
                "The Agent approval request is outside the supported bounds."
            }
            ErrorCodeV1::AgentApprovalUnavailable => {
                "The exact Agent approval could not be inspected or controlled safely."
            }
            ErrorCodeV1::InvalidAgentSessionRequest => {
                "The Agent conversation request is outside the supported bounds."
            }
            ErrorCodeV1::AgentSessionRevisionConflict => {
                "The Agent conversation changed. Reload it before continuing."
            }
            ErrorCodeV1::AgentSessionUnavailable => {
                "The Agent conversation could not be completed safely."
            }
            ErrorCodeV1::AgentSessionBusy => {
                "This Agent conversation is already processing another action."
            }
            ErrorCodeV1::IndexRebuildAlreadyPending => {
                "An index rebuild is already queued or running for the active worktree."
            }
            ErrorCodeV1::IndexRebuildUnavailable => {
                "The local index coordinator could not accept the rebuild request."
            }
            ErrorCodeV1::ProjectOperationBusy => "Another project operation is still in progress.",
            ErrorCodeV1::InvalidProjectCatalogRequest => {
                "The project catalog request is outside the supported bounds."
            }
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
            ErrorCodeV1::InvalidSettingsRequest => {
                "The Settings request is outside the supported bounds or no longer current."
            }
            ErrorCodeV1::ModelEndpointInvalid => {
                "The model endpoint is invalid, unsafe, or not authorized for this probe."
            }
            ErrorCodeV1::ModelProbeAlreadyActive => {
                "Another explicit model discovery or capability operation is already running."
            }
            ErrorCodeV1::ModelSettingsUnavailable => {
                "Local model settings could not be read, stored, or probed safely."
            }
            ErrorCodeV1::ProviderCredentialInvalid => {
                "The provider credential does not satisfy the supported bounds."
            }
            ErrorCodeV1::ProviderCredentialMissing => {
                "The active provider requires an API key before this operation."
            }
            ErrorCodeV1::ProviderCredentialRecoveryRequired => {
                "The provider credential must be replaced or deleted before use."
            }
            ErrorCodeV1::ProviderCredentialStoreUnavailable => {
                "The operating-system credential store is unavailable or locked."
            }
            ErrorCodeV1::InvalidProjectSettingsRequest => {
                "The project Settings request is invalid or no longer current."
            }
            ErrorCodeV1::ProjectSettingsUnavailable => {
                "Project ignore or command Settings could not be processed safely."
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

    /// Creates a safe global Settings or model-probe failure.
    #[must_use]
    pub fn settings(code: ErrorCodeV1) -> Self {
        Self::project_open(code)
    }

    /// Creates a safe active-project ignore or command Settings failure.
    #[must_use]
    pub fn project_settings(code: ErrorCodeV1) -> Self {
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
