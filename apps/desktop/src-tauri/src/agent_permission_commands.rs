use crate::CompositionRoot;
use a3_domain::{AgentPermissionMode, AgentPermissionRevision};
use a3_protocol::{AgentPermissionModeV1, AgentPermissionsResponseV1, CommandErrorV1, ErrorCodeV1};

fn project(settings: a3_domain::AgentPermissionSettings) -> AgentPermissionsResponseV1 {
    AgentPermissionsResponseV1::new(
        settings.revision().get().to_string(),
        match settings.mode() {
            AgentPermissionMode::AskPermissions => AgentPermissionModeV1::AskPermissions,
            AgentPermissionMode::FullMachine => AgentPermissionModeV1::FullMachine,
        },
    )
}

fn failure(error: a3_application::AgentPermissionStoreFailure) -> CommandErrorV1 {
    CommandErrorV1::agent_permissions(match error {
        a3_application::AgentPermissionStoreFailure::Conflict => {
            ErrorCodeV1::AgentPermissionsChanged
        }
        a3_application::AgentPermissionStoreFailure::InvalidStoredData => {
            ErrorCodeV1::LocalStorageInvalidData
        }
        a3_application::AgentPermissionStoreFailure::Unavailable => {
            ErrorCodeV1::AgentPermissionsUnavailable
        }
    })
}

impl CompositionRoot {
    /// Reads app-wide durable permissions without starting work.
    pub async fn query_agent_permissions(
        &self,
    ) -> Result<AgentPermissionsResponseV1, CommandErrorV1> {
        let store = self
            .agent_permissions
            .as_ref()
            .ok_or_else(|| failure(a3_application::AgentPermissionStoreFailure::Unavailable))?;
        store
            .load_agent_permissions()
            .await
            .map(project)
            .map_err(failure)
    }

    /// The user's explicit mode selection is the only entry point that changes permissions.
    pub async fn update_agent_permissions(
        &self,
        revision: AgentPermissionRevision,
        mode: AgentPermissionMode,
    ) -> Result<AgentPermissionsResponseV1, CommandErrorV1> {
        let store = self
            .agent_permissions
            .as_ref()
            .ok_or_else(|| failure(a3_application::AgentPermissionStoreFailure::Unavailable))?;
        let settings = store
            .update_agent_permissions(revision, mode)
            .await
            .map_err(failure)?;
        if mode == AgentPermissionMode::FullMachine
            && let Some(manager) = &self.agent_run_manager
        {
            // Owned coordinator retries only while its preceding attempt is winding down.
            // A persisted mode selection succeeds even if no runnable task exists.
            let _wake = manager.permissions_changed();
        }
        Ok(project(settings))
    }
}
