use crate::{MachineFileAction, MachineHttpAction, MachineProcessAction, TaskStepId};

/// Closed V1 machine tool proposals carried by AgentAction V6. Core owns every permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentMachineAction {
    /// Complete, bounded, hash-protected external file operation.
    File(MachineFileAction),
    /// Additional direct argv process; no model-authored safety or environment fields.
    Process(MachineProcessAction),
    /// Exact credential-free HTTP GET; no headers, body or redirect policy fields.
    HttpGet(MachineHttpAction),
}
impl AgentMachineAction {
    /// Returns the owning step, independently checked against current Core turn anchors.
    #[must_use]
    pub const fn step_id(&self) -> TaskStepId {
        match self {
            Self::File(a) => a.step_id(),
            Self::Process(a) => a.step_id(),
            Self::HttpGet(a) => a.step_id(),
        }
    }
}
