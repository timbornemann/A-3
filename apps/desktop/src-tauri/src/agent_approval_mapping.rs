use crate::encode_hex;
use a3_application::{
    AgentApprovalAction, AgentApprovalCenter, AgentApprovalFileOperation,
    AgentApprovalNetworkScope, AgentApprovalWorkingDirectory, AgentProcessInspectionKind,
};
use a3_domain::{
    ActionClass, PolicyDecisionReason, ProcessExecutionMode, ProcessPlanBinding, RepositoryPath,
    RiskLevel,
};
use a3_protocol::{
    AgentApprovalActionClassV1, AgentApprovalActionV1, AgentApprovalExecutionModeV1,
    AgentApprovalFileOperationV1, AgentApprovalFileV1, AgentApprovalNetworkV1,
    AgentApprovalPatchV1, AgentApprovalPlanBindingV1, AgentApprovalProcessV1,
    AgentApprovalReasonV1, AgentApprovalRiskV1, AgentApprovalStatusV1, AgentApprovalV1,
    AgentApprovalWorkingDirectoryV1, AgentControllerStateV1, AgentInspectionPathV1,
    AgentInspectionProcessKindV1, TaskLensStepStatusV1,
};

pub(crate) fn map_agent_approval_to_v1(value: &AgentApprovalCenter) -> AgentApprovalV1 {
    let presentation = value.presentation();
    let context = presentation.context();
    AgentApprovalV1::new(
        presentation.revision().get().to_string(),
        value.ledger_revision(),
        value.ledger_store_version().get().to_string(),
        map_state(value.controller_state()),
        map_step_status(value.step_status()),
        context.step_id().to_string(),
        context.snapshot_id().to_string(),
        encode_hex(presentation.scope_digest().as_bytes()),
        map_class(presentation.action_class()),
        map_risk(presentation.risk_level()),
        map_reason(presentation.reason()),
        presentation.requested_at().unix_millis().to_string(),
        presentation.expires_at().unix_millis().to_string(),
        match value.status() {
            a3_application::AgentApprovalStatus::Pending => AgentApprovalStatusV1::Pending,
            a3_application::AgentApprovalStatus::Active => AgentApprovalStatusV1::Active,
            a3_application::AgentApprovalStatus::Consumed => AgentApprovalStatusV1::Consumed,
            a3_application::AgentApprovalStatus::Revoked => AgentApprovalStatusV1::Revoked,
            a3_application::AgentApprovalStatus::Expired => AgentApprovalStatusV1::Expired,
            a3_application::AgentApprovalStatus::Denied => AgentApprovalStatusV1::Denied,
        },
        map_action(presentation.action()),
        value.can_allow_once(),
        value.can_deny(),
        value.can_continue(),
        value.can_revoke(),
    )
}

fn map_action(value: &AgentApprovalAction) -> AgentApprovalActionV1 {
    match value {
        AgentApprovalAction::Patch(patch) => AgentApprovalActionV1::Patch {
            patch: AgentApprovalPatchV1::new(
                patch.rationale().to_owned(),
                patch
                    .files()
                    .iter()
                    .map(|file| {
                        AgentApprovalFileV1::new(
                            match file.operation() {
                                AgentApprovalFileOperation::Add => {
                                    AgentApprovalFileOperationV1::Add
                                }
                                AgentApprovalFileOperation::Update => {
                                    AgentApprovalFileOperationV1::Update
                                }
                                AgentApprovalFileOperation::Move => {
                                    AgentApprovalFileOperationV1::Move
                                }
                                AgentApprovalFileOperation::Delete => {
                                    AgentApprovalFileOperationV1::Delete
                                }
                            },
                            file.source_path().map(map_path),
                            file.target_path().map(map_path),
                        )
                    })
                    .collect(),
            ),
        },
        AgentApprovalAction::Process(process) => AgentApprovalActionV1::Process {
            process: AgentApprovalProcessV1::new(
                map_process_kind(process.kind()),
                process.executable().to_owned(),
                process.arguments().to_vec(),
                match process.working_directory() {
                    AgentApprovalWorkingDirectory::Root => AgentApprovalWorkingDirectoryV1::Root,
                    AgentApprovalWorkingDirectory::Subtree(path) => {
                        AgentApprovalWorkingDirectoryV1::Subtree {
                            path: map_path(path),
                        }
                    }
                },
                process.environment_allowlist().to_vec(),
                process.timeout_millis().to_string(),
                process.stdout_limit(),
                process.stderr_limit(),
                match process.execution_mode() {
                    ProcessExecutionMode::KnownSafe => AgentApprovalExecutionModeV1::KnownSafe,
                    ProcessExecutionMode::Open => AgentApprovalExecutionModeV1::Open,
                    ProcessExecutionMode::Shell => AgentApprovalExecutionModeV1::Shell,
                },
                match process.plan_binding() {
                    ProcessPlanBinding::Unbound => AgentApprovalPlanBindingV1::Unbound,
                    ProcessPlanBinding::Validated(step_id) => {
                        AgentApprovalPlanBindingV1::Validated {
                            step_id: step_id.to_string(),
                        }
                    }
                },
                match process.network() {
                    AgentApprovalNetworkScope::Denied => AgentApprovalNetworkV1::Denied,
                    AgentApprovalNetworkScope::Requested(scope) => {
                        AgentApprovalNetworkV1::Requested {
                            scope_digest: scope.to_string(),
                        }
                    }
                },
                process.specification_id().to_string(),
            ),
        },
    }
}

fn map_path(value: &RepositoryPath) -> AgentInspectionPathV1 {
    AgentInspectionPathV1::new(
        encode_hex(value.as_bytes()),
        String::from_utf8_lossy(value.as_bytes())
            .chars()
            .flat_map(char::escape_default)
            .collect(),
    )
}

const fn map_process_kind(value: AgentProcessInspectionKind) -> AgentInspectionProcessKindV1 {
    match value {
        AgentProcessInspectionKind::Test => AgentInspectionProcessKindV1::Test,
        AgentProcessInspectionKind::Build => AgentInspectionProcessKindV1::Build,
        AgentProcessInspectionKind::Diagnostic => AgentInspectionProcessKindV1::Diagnostic,
        AgentProcessInspectionKind::Lint => AgentInspectionProcessKindV1::Lint,
        AgentProcessInspectionKind::Format => AgentInspectionProcessKindV1::Format,
        AgentProcessInspectionKind::Command => AgentInspectionProcessKindV1::Command,
    }
}

const fn map_class(value: ActionClass) -> AgentApprovalActionClassV1 {
    match value {
        ActionClass::Read => AgentApprovalActionClassV1::Read,
        ActionClass::Derive => AgentApprovalActionClassV1::Derive,
        ActionClass::Write => AgentApprovalActionClassV1::Write,
        ActionClass::ExecuteSafe => AgentApprovalActionClassV1::ExecuteSafe,
        ActionClass::ExecuteOpen => AgentApprovalActionClassV1::ExecuteOpen,
        ActionClass::Network => AgentApprovalActionClassV1::Network,
        ActionClass::Destructive => AgentApprovalActionClassV1::Destructive,
        ActionClass::Publish => AgentApprovalActionClassV1::Publish,
        ActionClass::OutsideRoot => AgentApprovalActionClassV1::OutsideRoot,
    }
}

const fn map_risk(value: RiskLevel) -> AgentApprovalRiskV1 {
    match value {
        RiskLevel::Low => AgentApprovalRiskV1::Low,
        RiskLevel::Moderate => AgentApprovalRiskV1::Moderate,
        RiskLevel::High => AgentApprovalRiskV1::High,
        RiskLevel::Critical => AgentApprovalRiskV1::Critical,
    }
}

const fn map_reason(value: PolicyDecisionReason) -> AgentApprovalReasonV1 {
    match value {
        PolicyDecisionReason::WorkspaceApprovalRequired => AgentApprovalReasonV1::WorkspacePolicy,
        PolicyDecisionReason::SystemApprovalRequired
        | PolicyDecisionReason::SystemAutomatic
        | PolicyDecisionReason::WorkspaceDenied
        | PolicyDecisionReason::ApprovalGranted
        | PolicyDecisionReason::ApprovalRunMismatch
        | PolicyDecisionReason::ApprovalScopeMismatch
        | PolicyDecisionReason::ApprovalActionMismatch
        | PolicyDecisionReason::ApprovalExpired
        | PolicyDecisionReason::ApprovalRevoked
        | PolicyDecisionReason::ApprovalAlreadyConsumed
        | PolicyDecisionReason::ApprovalTimestampRegressed => AgentApprovalReasonV1::SystemPolicy,
    }
}

const fn map_state(value: a3_domain::AgentControllerState) -> AgentControllerStateV1 {
    match value {
        a3_domain::AgentControllerState::Intake => AgentControllerStateV1::Intake,
        a3_domain::AgentControllerState::Localize => AgentControllerStateV1::Localize,
        a3_domain::AgentControllerState::Plan => AgentControllerStateV1::Plan,
        a3_domain::AgentControllerState::Execute => AgentControllerStateV1::Execute,
        a3_domain::AgentControllerState::Verify => AgentControllerStateV1::Verify,
        a3_domain::AgentControllerState::Replan => AgentControllerStateV1::Replan,
        a3_domain::AgentControllerState::AwaitApproval => AgentControllerStateV1::AwaitApproval,
        a3_domain::AgentControllerState::Done => AgentControllerStateV1::Done,
        a3_domain::AgentControllerState::Failed => AgentControllerStateV1::Failed,
        a3_domain::AgentControllerState::Cancelled => AgentControllerStateV1::Cancelled,
    }
}

const fn map_step_status(value: a3_domain::TaskStepStatus) -> TaskLensStepStatusV1 {
    match value {
        a3_domain::TaskStepStatus::Pending => TaskLensStepStatusV1::Pending,
        a3_domain::TaskStepStatus::Ready => TaskLensStepStatusV1::Ready,
        a3_domain::TaskStepStatus::InProgress => TaskLensStepStatusV1::InProgress,
        a3_domain::TaskStepStatus::Blocked => TaskLensStepStatusV1::Blocked,
        a3_domain::TaskStepStatus::AwaitingApproval => TaskLensStepStatusV1::AwaitingApproval,
        a3_domain::TaskStepStatus::Verifying => TaskLensStepStatusV1::Verifying,
        a3_domain::TaskStepStatus::Completed => TaskLensStepStatusV1::Completed,
        a3_domain::TaskStepStatus::Failed => TaskLensStepStatusV1::Failed,
        a3_domain::TaskStepStatus::Cancelled => TaskLensStepStatusV1::Cancelled,
        a3_domain::TaskStepStatus::Stale => TaskLensStepStatusV1::Stale,
    }
}
