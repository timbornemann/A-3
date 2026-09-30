//! Closed machine effects within the existing finite controller and worktree lease.
use super::*;
use crate::{
    AuthorizedMachineFileAction, AuthorizedMachineHttpGet, MachineEffectScope, MachineFileControl,
    MachineFileToolFailure, MachineNetworkFailure, PreparedMachineFileAction,
};
use a3_domain::{
    AgentMachineAction, MachineProcessEffect, PolicyAction, PreparedMachineHttpAction, ProcessSpec,
};

#[derive(Debug)]
enum PreparedTool {
    File(PreparedMachineFileAction),
    Process(ProcessSpec),
    Http(PreparedMachineHttpAction),
}

#[derive(Debug)]
pub(super) struct PreparedMachineMutation {
    step: TaskStepId,
    scope: MachineEffectScope,
    tool: PreparedTool,
}
impl PreparedMachineMutation {
    pub(super) const fn step_id(&self) -> TaskStepId {
        self.step
    }
    pub(super) const fn kind(&self) -> AgentMutationKind {
        self.scope.kind()
    }
    pub(super) const fn scope(&self) -> &MachineEffectScope {
        &self.scope
    }
    pub(super) fn policy_action(&self) -> PolicyAction {
        match &self.tool {
            PreparedTool::File(file) => file.policy_action().clone(),
            PreparedTool::Process(spec) => spec.policy_action(),
            PreparedTool::Http(http) => http.policy_action(),
        }
    }
    pub(super) fn record_approval(
        &self,
        sink: &dyn AgentApprovalSink,
        project: &ProjectIdentity,
        context: AgentInspectionContext,
        request: &a3_domain::ApprovalRequest,
        reason: a3_domain::PolicyDecisionReason,
    ) -> Result<(), AgentApprovalSinkFailure> {
        match &self.tool {
            PreparedTool::Process(spec) => {
                sink.record_process_request(
                    project,
                    context,
                    request,
                    reason,
                    AgentProcessInspectionKind::Command,
                    spec,
                )?;
            }
            _ => {
                sink.record_machine_request(
                    project,
                    context,
                    request,
                    reason,
                    &self.policy_action(),
                    &self.scope,
                )?;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
struct FileControl<'a>(&'a dyn ProcessRunControl);
impl MachineFileControl for FileControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

impl ExecuteMutatingAgentAction<'_> {
    pub(super) async fn prepare_machine<C: ProcessRunControl>(
        self,
        project: &ProjectIdentity,
        run: &AgentRun,
        ledger: &TaskLedger,
        published: &PublishedIndex,
        action: AgentMachineAction,
        control: &C,
    ) -> Result<PreparedMachineMutation, MutationControllerFailure> {
        let step = action.step_id();
        let expected = if run.state() == AgentControllerState::AwaitApproval {
            TaskStepStatus::AwaitingApproval
        } else {
            TaskStepStatus::InProgress
        };
        if !matches!(
            run.state(),
            AgentControllerState::Execute | AgentControllerState::AwaitApproval
        ) || run.current_snapshot_id() != published.run().snapshot_id()
            || run.goal_contract() != ledger.goal_contract()
            || run.task_ledger_revision() != ledger.revision()
            || !ledger.step(step).is_some_and(|s| {
                s.status() == expected
                    && s.attempts().last().is_some_and(|a| a.run_id() == run.id())
            })
        {
            return Err(MutationControllerFailure::AnchorMismatch);
        }
        let invalid = |_| MutationControllerFailure::MachinePreparation;
        let (tool, kind, resource, target, expected, proposed) = match action {
            AgentMachineAction::File(file) => {
                let prepared = self
                    .machine_files
                    .ok_or(MutationControllerFailure::MachinePreparation)?
                    .prepare(project, &file, &FileControl(control))
                    .await
                    .map_err(invalid)?;
                let name = std::str::from_utf8(prepared.file().as_bytes())
                    .map_err(|_| MutationControllerFailure::MachinePreparation)?;
                let target = prepared
                    .root()
                    .as_path()
                    .join(name)
                    .to_str()
                    .ok_or(MutationControllerFailure::MachinePreparation)?
                    .to_owned();
                let PolicyAction::MachineFile {
                    resource_id,
                    expected,
                    proposed,
                    ..
                } = *prepared.policy_action()
                else {
                    return Err(MutationControllerFailure::MachinePreparation);
                };
                (
                    PreparedTool::File(prepared),
                    AgentMutationKind::MachineFile,
                    resource_id,
                    target,
                    expected,
                    proposed,
                )
            }
            AgentMachineAction::Process(action) => {
                let spec = crate::PrepareMachineProcess
                    .execute(project, run, ledger, &action, self.machine_environment)
                    .map_err(|_| MutationControllerFailure::MachinePreparation)?;
                let resource = spec.specification_id();
                let target = spec.executable().as_str().to_owned();
                (
                    PreparedTool::Process(spec),
                    AgentMutationKind::MachineProcess,
                    resource,
                    target,
                    None,
                    None,
                )
            }
            AgentMachineAction::HttpGet(action) => {
                let http = self
                    .machine_network
                    .ok_or(MutationControllerFailure::MachinePreparation)?
                    .prepare(project, &action)
                    .map_err(|_| MutationControllerFailure::MachinePreparation)?;
                let resource = http.resource();
                let target = http.url().as_str().to_owned();
                (
                    PreparedTool::Http(http),
                    AgentMutationKind::MachineNetwork,
                    resource,
                    target,
                    None,
                    None,
                )
            }
        };
        let scope = MachineEffectScope::new(kind, step, resource, target, expected, proposed)
            .map_err(|_| MutationControllerFailure::MachinePreparation)?;
        Ok(PreparedMachineMutation { step, scope, tool })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_machine<C>(
        self,
        project: &ProjectIdentity,
        run: &mut AgentRun,
        ledger: &mut TaskLedger,
        ledger_version: &mut TaskLedgerStoreVersion,
        step: TaskStepId,
        prepared: PreparedMachineMutation,
        decision: &PolicyDecision,
        ids: MutationExecutionIds,
        observed_at: AgentRunTimestamp,
        context_seed: &MutationContextSeed,
        index_compiler: &mut dyn RepositoryIndexCompiler,
        events: &dyn ProcessEventSink,
        control: &C,
        attempt: a3_domain::AgentToolAttemptNumber,
        lease: &crate::WorktreeMutationLease<'_>,
    ) -> Result<MutationControllerOutcome, MutationControllerFailure>
    where
        C: AgentControllerControl
            + ContextCompileControl
            + ProcessRunControl
            + RepositoryIndexControl,
    {
        // The durable Unknown attempt already exists. Errors after this point are never retried
        // without classifying whether the boundary could have been crossed.
        let before = run.current_snapshot_id();
        let mut after = before;
        let outcome: Result<String, AgentMutationDisposition> = match prepared.tool {
            PreparedTool::File(file) => {
                let result = match AuthorizedMachineFileAction::new(file, decision) {
                    Ok(authorized) => {
                        self.machine_files
                            .ok_or(MutationControllerFailure::MachinePreparation)?
                            .execute(authorized, &FileControl(control))
                            .await
                    }
                    Err(error) => Err(error),
                };
                result.map(|receipt| format!("External file observation (attempt started at {}): resource={}, observed_hash={}, changed={}. {}",
                    observed_at.unix_millis(), receipt.resource, machine_hash(receipt.hash), receipt.changed,
                    receipt.text.map(|text| String::from_utf8_lossy(text.as_bytes()).into_owned()).unwrap_or_default()))
                    .map_err(|error| match error {
                        MachineFileToolFailure::ReconciliationRequired => AgentMutationDisposition::Unknown(MutationReconciliation::Required),
                        _ => AgentMutationDisposition::NotApplied,
                    })
            }
            PreparedTool::Http(http) => {
                let result = match AuthorizedMachineHttpGet::new(http, run.id(), decision) {
                    Ok(authorized) => {
                        self.machine_network
                            .ok_or(MutationControllerFailure::MachinePreparation)?
                            .get(authorized, control)
                            .await
                    }
                    Err(error) => Err(error),
                };
                result.map(|receipt| format!("HTTP observation (attempt started at {}): resource={}, status={}, body_hash={}. {}",
                    observed_at.unix_millis(), receipt.resource, receipt.status, machine_hash(Some(receipt.hash)), String::from_utf8_lossy(receipt.body.as_bytes())))
                    .map_err(|error| if error == MachineNetworkFailure::PermissionsChanged {
                        AgentMutationDisposition::NotApplied
                    } else { AgentMutationDisposition::Unknown(MutationReconciliation::Required) })
            }
            PreparedTool::Process(spec) => {
                let bounded_read = spec.machine_effect() == Some(MachineProcessEffect::ReadOnly);
                let result = self
                    .process_runner
                    .run(
                        project,
                        crate::AuthorizedProcessSpec::new(spec, decision)?,
                        control,
                        events,
                    )
                    .await;
                match result {
                    Err(error) => Err(map_process_failure_disposition(error)),
                    Ok(result) => {
                        self.observe_process_result(
                            project,
                            run,
                            step,
                            current_step_spec(ledger, step)?,
                            ids.tool_run_id,
                            AgentProcessInspectionKind::Command,
                            before,
                            &result,
                        );
                        if !bounded_read
                            || !matches!(result.termination(), ProcessTermination::Exited(_))
                        {
                            // Exit status cannot prove the effects of an arbitrary script. Preserve
                            // Unknown and force scoped user recovery instead of adopting a project scan.
                            Err(AgentMutationDisposition::Unknown(
                                MutationReconciliation::Required,
                            ))
                        } else {
                            let batch = RepositoryChangeBatch::full_rescan(
                                Vec::new(),
                                crate::RepositoryRescanReason::Explicit,
                            )?;
                            match self
                                .refresh
                                .execute(project, &batch, index_compiler, control)
                                .await
                            {
                                Ok(refresh) => {
                                    after = refresh.published_index().run().snapshot_id();
                                    Ok(format!(
                                        "Process observation (attempt started at {}): termination={:?}; stdout={}; stderr={}",
                                        observed_at.unix_millis(),
                                        result.termination(),
                                        result.stdout().content().as_text().unwrap_or("[redacted]"),
                                        result.stderr().content().as_text().unwrap_or("[redacted]")
                                    ))
                                }
                                Err(_) => Err(AgentMutationDisposition::Unknown(
                                    MutationReconciliation::Required,
                                )),
                            }
                        }
                    }
                }
            }
        };
        let text = match outcome {
            Ok(text) => text,
            Err(disposition) => {
                self.recovery
                    .finish_agent_mutation_attempt(
                        project,
                        ids.tool_run_id,
                        attempt,
                        AgentToolAttemptStatus::Failed,
                        disposition,
                        observed_at,
                    )
                    .await
                    .map_err(MutationControllerFailure::MutationResultStore)?;
                self.record_tool_event(
                    project,
                    run,
                    ids.tool_event_id,
                    ids.tool_run_id,
                    before,
                    false,
                    None,
                    observed_at,
                )
                .await?;
                if disposition.requires_reconciliation() {
                    return Ok(MutationControllerOutcome::ReconciliationRequired {
                        tool_run_id: ids.tool_run_id,
                        attempt,
                        snapshot_id: before,
                    });
                }
                return self
                    .resolve_unverified_failure(
                        project,
                        run,
                        ledger,
                        ledger_version,
                        step,
                        ids,
                        observed_at,
                        context_seed,
                        control,
                        lease,
                        MutationFailureClass::VerificationFailed,
                    )
                    .await;
            }
        };
        let digest = ContextToolResultDigest::from_bytes(*blake3::hash(text.as_bytes()).as_bytes());
        let result = AgentMutationResultRecord::new(digest, false, text.len() as u64);
        self.record_successful_mutation_event(
            project,
            run,
            ids.tool_event_id,
            ids.tool_run_id,
            attempt,
            after,
            None,
            result,
            observed_at,
        )
        .await?;
        let (preview, truncated) = bounded_preview(&text);
        let tool = ContextToolResult::new(
            run.last_event_sequence(),
            ids.tool_run_id,
            crate::ContextToolResultStatus::Succeeded,
            crate::ContextToolResultPreview::try_from_string(preview)
                .map_err(|_| MutationControllerFailure::InvalidToolResult)?,
            digest,
            truncated,
            before,
            after,
        );
        let mut seed = context_seed.clone();
        seed.tool_results.push(tool.clone());
        if seed.tool_results.len() > 64 {
            seed.tool_results.remove(0);
        }
        // Machine observations do not complete a repository verification specification.
        self.request_next_execution(project, run, ledger, step, ids, observed_at, &seed, control)
            .await?;
        lease.record_success();
        Ok(MutationControllerOutcome::MachineObserved(Box::new(tool)))
    }
}

fn machine_hash(hash: Option<a3_domain::ContentHash>) -> String {
    hash.map_or_else(
        || "absent".to_owned(),
        |hash| {
            hash.as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        },
    )
}

fn bounded_preview(text: &str) -> (String, bool) {
    let mut end = text.len().min(2048);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), end < text.len())
}
