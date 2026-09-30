use crate::ProtocolVersion;
use serde::{Deserialize, Serialize};

/// Opaque selector; the WebView cannot supply a path or executable.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentMachineRecoveryV1 {
    /// Narrow contract version.
    pub protocol_version: ProtocolVersion,
    /// Durable selected task.
    pub task_id: String,
}

/// Human-only scope acknowledgement, never an AgentAction.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecoverAgentMachineEffectV1 {
    /// Narrow contract version.
    pub protocol_version: ProtocolVersion,
    /// Durable selected task.
    pub task_id: String,
    /// Exact current plan revision.
    pub expected_ledger_revision: u32,
    /// Exact current storage CAS revision.
    pub expected_ledger_store_version: String,
    /// Fingerprint of the displayed Unknown action, not executable content.
    pub expected_scope: String,
    /// Explicit consent to read a known file and acknowledge the displayed uncertainty.
    pub action: MachineRecoveryActionV1,
}

/// Closed user consent matching the displayed effect kind.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MachineRecoveryActionV1 {
    /// Authorize only the displayed file read, then require a new plan.
    ObserveFileAndReplan,
    /// Acknowledge unobservable process/network effects, then require a new plan.
    AcknowledgeUnknownAndReplan,
}

/// Bounded resource-kind presentation.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MachineRecoveryKindV1 {
    /// Complete external file.
    File,
    /// Additional argv process with unbounded potential effects.
    Process,
    /// HTTP operation which must never be replayed for recovery.
    Http,
}

/// Exact bounded scope selected by the Core from durable history.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMachineRecoveryScopeV1 {
    /// Current plan revision.
    pub ledger_revision: u32,
    /// Current CAS revision.
    pub ledger_store_version: String,
    /// Exact Unknown action fingerprint.
    pub scope: String,
    /// Core-owned resource kind.
    pub resource_kind: MachineRecoveryKindV1,
    /// Canonical file/URL or program name; no source content or environment values.
    pub target: String,
}

/// Narrow query/control response; failure never clears the Unknown fence.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum AgentMachineRecoveryResultV1 {
    /// No matching Unknown machine scope or current task exists.
    Unavailable,
    /// User can inspect the exact resource before choosing recovery.
    Available {
        /// Exact bounded displayed scope.
        recovery: AgentMachineRecoveryScopeV1,
    },
    /// An owned cancellable recovery job was admitted under current anchors.
    Queued,
    /// The displayed scope changed; reload before deciding.
    ActivityChanged,
}

/// Version-one response envelope.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMachineRecoveryResponseV1 {
    protocol_version: ProtocolVersion,
    result: AgentMachineRecoveryResultV1,
}
impl AgentMachineRecoveryResponseV1 {
    /// Constructs a typed Core result.
    #[must_use]
    pub const fn new(result: AgentMachineRecoveryResultV1) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            result,
        }
    }
}
