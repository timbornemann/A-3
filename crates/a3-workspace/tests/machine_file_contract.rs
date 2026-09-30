//! Actual external-file effects with exact central authorization and current permissions.
mod support;

use a3_application::{
    AgentPermissionFuture, AgentPermissionStore, AgentPermissionStoreFailure,
    AuthorizedMachineFileAction, MachineFileControl, MachineFileTool, MachineFileToolFailure,
};
use a3_domain::*;
use a3_workspace::WorkspaceMachineFileTool;
use std::{
    error::Error,
    fs,
    path::Path,
    sync::{Arc, Mutex},
};
use support::TempDirectory;

#[derive(Debug)]
struct Permissions(Mutex<AgentPermissionSettings>);
impl AgentPermissionStore for Permissions {
    fn load_agent_permissions(&self) -> AgentPermissionFuture<'_> {
        Box::pin(async {
            self.0
                .lock()
                .map(|value| *value)
                .map_err(|_| AgentPermissionStoreFailure::Unavailable)
        })
    }
    fn update_agent_permissions(
        &self,
        expected: AgentPermissionRevision,
        mode: AgentPermissionMode,
    ) -> AgentPermissionFuture<'_> {
        Box::pin(async move {
            let mut value = self
                .0
                .lock()
                .map_err(|_| AgentPermissionStoreFailure::Unavailable)?;
            if value.revision() != expected {
                return Err(AgentPermissionStoreFailure::Conflict);
            }
            *value = AgentPermissionSettings::new(
                mode,
                expected
                    .next()
                    .map_err(|_| AgentPermissionStoreFailure::Unavailable)?,
            );
            Ok(*value)
        })
    }
}
#[derive(Debug)]
struct Active;
impl MachineFileControl for Active {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[test]
fn external_read_write_delete_and_revision_changes_are_exact() -> Result<(), Box<dyn Error>> {
    futures::executor::block_on(async {
        let fixture = TempDirectory::new()?;
        let project_root = fixture.path().join("project");
        let outside = fixture.path().join("outside");
        fs::create_dir(&project_root)?;
        fs::create_dir(&outside)?;
        let project = project(&project_root)?;
        let full = AgentPermissionSettings::new(
            AgentPermissionMode::FullMachine,
            AgentPermissionRevision::new(2)?,
        );
        let permissions = Arc::new(Permissions(Mutex::new(full)));
        let adapter = WorkspaceMachineFileTool::new(permissions.clone());
        let target = outside.join("result.txt");
        let proposal = |operation| -> Result<MachineFileAction, Box<dyn Error>> {
            Ok(MachineFileAction::new(
                TaskStepId::from_bytes([4; 32]),
                MachineFilePath::new(target.to_str().ok_or("invalid fixture path")?.to_owned())?,
                operation,
            )?)
        };
        let content = PatchFileContent::try_from_bytes(b"hello machine\n".to_vec())?;
        let create = proposal(MachineFileOperation::Write {
            expected: None,
            content: content.clone(),
        })?;
        let prepared = adapter.prepare(&project, &create, &Active).await?;
        assert_eq!(
            AgentPermissionSettings::INITIAL.disposition(prepared.policy_action()),
            PolicyDisposition::ApprovalRequired
        );
        assert!(
            AuthorizedMachineFileAction::new(
                prepared.clone(),
                &automatic(prepared.policy_action(), AgentPermissionSettings::INITIAL)?
            )
            .is_err()
        );
        let authorized = AuthorizedMachineFileAction::new(
            prepared.clone(),
            &automatic(prepared.policy_action(), full)?,
        )?;
        let receipt = adapter.execute(authorized, &Active).await?;
        assert!(receipt.changed);
        assert_eq!(receipt.hash, Some(content.content_hash()));
        assert_eq!(fs::read(&target)?, content.as_bytes());

        let read = proposal(MachineFileOperation::Read(Some(content.content_hash())))?;
        let prepared = adapter.prepare(&project, &read, &Active).await?;
        let receipt = adapter
            .execute(
                AuthorizedMachineFileAction::new(
                    prepared.clone(),
                    &automatic(prepared.policy_action(), full)?,
                )?,
                &Active,
            )
            .await?;
        assert!(!receipt.changed);
        assert_eq!(
            receipt.text.as_ref().map(PatchFileContent::as_bytes),
            Some(content.as_bytes())
        );

        let replacement = PatchFileContent::try_from_bytes(b"new value\n".to_vec())?;
        let update = proposal(MachineFileOperation::Write {
            expected: Some(content.content_hash()),
            content: replacement.clone(),
        })?;
        let prepared = adapter.prepare(&project, &update, &Active).await?;
        let authorized = AuthorizedMachineFileAction::new(
            prepared.clone(),
            &automatic(prepared.policy_action(), full)?,
        )?;
        permissions
            .update_agent_permissions(full.revision(), AgentPermissionMode::AskPermissions)
            .await?;
        assert!(matches!(
            adapter.execute(authorized, &Active).await,
            Err(MachineFileToolFailure::PermissionsChanged)
        ));
        assert_eq!(fs::read(&target)?, content.as_bytes());

        let exact = approved(prepared.policy_action())?;
        let receipt = adapter
            .execute(AuthorizedMachineFileAction::new(prepared, &exact)?, &Active)
            .await?;
        assert_eq!(receipt.hash, Some(replacement.content_hash()));
        let stale = adapter.prepare(&project, &update, &Active).await?;
        assert!(matches!(
            adapter
                .execute(
                    AuthorizedMachineFileAction::new(
                        stale.clone(),
                        &approved(stale.policy_action())?
                    )?,
                    &Active
                )
                .await,
            Err(MachineFileToolFailure::Stale)
        ));
        assert_eq!(fs::read(&target)?, replacement.as_bytes());

        let delete = proposal(MachineFileOperation::Delete(replacement.content_hash()))?;
        let prepared = adapter.prepare(&project, &delete, &Active).await?;
        assert_eq!(
            full.disposition(prepared.policy_action()),
            PolicyDisposition::ApprovalRequired
        );
        assert!(
            AuthorizedMachineFileAction::new(
                prepared.clone(),
                &automatic(prepared.policy_action(), full)?
            )
            .is_err()
        );
        let receipt = adapter
            .execute(
                AuthorizedMachineFileAction::new(
                    prepared.clone(),
                    &approved(prepared.policy_action())?,
                )?,
                &Active,
            )
            .await?;
        assert_eq!(receipt.hash, None);
        assert!(!target.exists());
        assert_eq!(fs::read_dir(&outside)?.count(), 0);
        Ok::<_, Box<dyn Error>>(())
    })
}

#[test]
fn machine_files_deny_secrets_directories_and_project_aliases() -> Result<(), Box<dyn Error>> {
    futures::executor::block_on(async {
        let fixture = TempDirectory::new()?;
        let project_root = fixture.path().join("project");
        fs::create_dir(&project_root)?;
        let project = project(&project_root)?;
        let adapter = WorkspaceMachineFileTool::new(Arc::new(Permissions(Mutex::new(
            AgentPermissionSettings::INITIAL,
        ))));
        fs::write(fixture.path().join(".env"), b"secret")?;
        fs::write(project_root.join("inside.txt"), b"project")?;
        for path in [
            fixture.path().join(".env"),
            project_root.clone(),
            project_root.join("inside.txt"),
        ] {
            let action = MachineFileAction::new(
                TaskStepId::from_bytes([4; 32]),
                MachineFilePath::new(path.to_str().ok_or("invalid fixture path")?.to_owned())?,
                MachineFileOperation::Read(None),
            )?;
            assert!(adapter.prepare(&project, &action, &Active).await.is_err());
        }
        assert_eq!(fs::read(project_root.join("inside.txt"))?, b"project");
        Ok::<_, Box<dyn Error>>(())
    })
}

fn automatic(
    action: &PolicyAction,
    settings: AgentPermissionSettings,
) -> Result<PolicyDecision, Box<dyn Error>> {
    let time = AgentRunTimestamp::from_unix_millis(100)?;
    Ok(PolicyDecision::automatic(
        PolicyDecisionId::from_bytes([8; 32]),
        AgentRunId::from_bytes([5; 32]),
        action,
        PolicyEvaluationTiming::new(time, time)?,
    )
    .with_permission_settings(settings))
}
fn approved(action: &PolicyAction) -> Result<PolicyDecision, Box<dyn Error>> {
    let time = AgentRunTimestamp::from_unix_millis(100)?;
    let run = AgentRunId::from_bytes([5; 32]);
    let id = PolicyDecisionId::from_bytes([8; 32]);
    let request = ApprovalRequest::new(
        ApprovalRequestId::from_bytes([9; 32]),
        run,
        action,
        time,
        AgentRunTimestamp::from_unix_millis(200)?,
    )?;
    let mut grant = ApprovalGrant::grant(ApprovalId::from_bytes([10; 32]), &request, time)?;
    grant.consume(id, run, action, time)?;
    Ok(PolicyDecision::approved(
        id,
        run,
        action,
        &grant,
        PolicyEvaluationTiming::new(time, time)?,
    )?)
}
fn project(root: &Path) -> Result<ProjectIdentity, Box<dyn Error>> {
    let root = CanonicalDirectory::from_canonicalized(fs::canonicalize(root)?)?;
    let repository = RepositoryId::from_bytes([1; 32]);
    Ok(ProjectIdentity::new(
        RepositoryIdentity::new(repository, root.clone(), None),
        WorktreeIdentity::new(
            WorktreeId::from_bytes([2; 32]),
            WorktreeAnchorId::from_bytes([3; 32]),
            repository,
            root,
        ),
        GitHead::Unborn {
            reference: GitReferenceName::try_from_full_name("refs/heads/main")?,
        },
    )?)
}
