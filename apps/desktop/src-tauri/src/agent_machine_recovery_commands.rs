use super::*;
use a3_protocol::{
    AgentMachineRecoveryResponseV1 as Response, AgentMachineRecoveryResultV1 as ResultKind,
    MachineRecoveryActionV1, MachineRecoveryKindV1, QueryAgentMachineRecoveryV1,
    RecoverAgentMachineEffectV1,
};

#[tauri::command]
/// Reads one exact durable Unknown scope; accepts no executable content.
pub async fn query_agent_machine_recovery(
    request: QueryAgentMachineRecoveryV1,
    root: tauri::State<'_, CompositionRoot>,
) -> Result<Response, CommandErrorV1> {
    if request.protocol_version != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let task = decode_stable_id(&request.task_id)
        .map(TaskId::from_bytes)
        .map_err(|()| agent_task_control_unavailable())?;
    let Some(_operation) = root.try_acquire_agent_task_operation() else {
        return Ok(Response::new(ResultKind::ActivityChanged));
    };
    if root
        .agent_run_manager
        .as_ref()
        .is_some_and(|manager| manager.activity().state().owns_live_worker())
    {
        return Ok(Response::new(ResultKind::Unavailable));
    }
    let active = lock_recovering_poison(&root.active_project).clone();
    let (Some(active), Some(executor)) = (active, &root.machine_recovery_executor) else {
        return Ok(Response::new(ResultKind::Unavailable));
    };
    Ok(Response::new(
        match executor
            .machine_recovery_scope(&active.project, task)
            .await
            .map_err(|_| agent_task_control_unavailable())?
        {
            Some(recovery) => ResultKind::Available { recovery },
            None => ResultKind::Unavailable,
        },
    ))
}

#[tauri::command]
/// Starts an owned cancellable recovery job from explicit human consent to the displayed scope.
pub async fn recover_agent_machine_effect(
    request: RecoverAgentMachineEffectV1,
    root: tauri::State<'_, CompositionRoot>,
) -> Result<Response, CommandErrorV1> {
    if request.protocol_version != ProtocolVersion::CURRENT {
        return Err(CommandErrorV1::unsupported_protocol_version());
    }
    let invalid = || agent_task_control_unavailable();
    let task = decode_stable_id(&request.task_id)
        .map(TaskId::from_bytes)
        .map_err(|()| invalid())?;
    let fingerprint = decode_stable_id(&request.expected_scope)
        .map(a3_domain::MutationActionFingerprint::from_bytes)
        .map_err(|()| invalid())?;
    let version_value = request
        .expected_ledger_store_version
        .parse::<u64>()
        .map_err(|_| invalid())?;
    if version_value.to_string() != request.expected_ledger_store_version {
        return Err(invalid());
    }
    let version = TaskLedgerStoreVersion::new(version_value).map_err(|_| invalid())?;
    let revision =
        TaskLedgerRevision::new(request.expected_ledger_revision).map_err(|_| invalid())?;
    let Some(_operation) = root.try_acquire_agent_task_operation() else {
        return Ok(Response::new(ResultKind::ActivityChanged));
    };
    let active = lock_recovering_poison(&root.active_project).clone();
    let (Some(active), Some(executor), Some(manager)) = (
        active,
        &root.machine_recovery_executor,
        &root.agent_run_manager,
    ) else {
        return Ok(Response::new(ResultKind::Unavailable));
    };
    if manager.activity().state().owns_live_worker() {
        return Ok(Response::new(ResultKind::ActivityChanged));
    }
    let Some(scope) = executor
        .machine_recovery_scope(&active.project, task)
        .await
        .map_err(|_| invalid())?
    else {
        return Ok(Response::new(ResultKind::Unavailable));
    };
    let correct_consent = matches!(
        (scope.resource_kind, request.action),
        (
            MachineRecoveryKindV1::File,
            MachineRecoveryActionV1::ObserveFileAndReplan
        ) | (
            MachineRecoveryKindV1::Process | MachineRecoveryKindV1::Http,
            MachineRecoveryActionV1::AcknowledgeUnknownAndReplan
        )
    );
    if !correct_consent
        || scope.scope != request.expected_scope
        || scope.ledger_revision != revision.get()
        || scope.ledger_store_version != request.expected_ledger_store_version
    {
        return Ok(Response::new(ResultKind::ActivityChanged));
    }
    manager
        .start_attempt(AgentRunExecutionRequest::after_machine_recovery(
            task,
            revision,
            version,
            fingerprint,
        ))
        .map_err(map_agent_run_manager_error_to_v1)?;
    Ok(Response::new(ResultKind::Queued))
}
