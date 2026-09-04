use crate::{AgentResearchDepthV1, AgentSessionModeV1, ProtocolVersion};
use serde::{Deserialize, Serialize};

/// Strict request for the built-in slash-command catalog.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryAgentSlashCommandsRequestV1 {
    protocol_version: ProtocolVersion,
    mode: AgentSessionModeV1,
}

impl QueryAgentSlashCommandsRequestV1 {
    #[must_use]
    /// Returns the protocol version.
    pub const fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }
    #[must_use]
    /// Returns the mode used to project command availability.
    pub const fn mode(&self) -> AgentSessionModeV1 {
        self.mode
    }
}

/// Presentation role of a built-in command entry.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentSlashCommandRoleV1 {
    /// Primary outcome profile.
    Primary,
    /// Optional specialist lens.
    Lens,
}

/// One Core-owned command palette entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSlashCommandV1 {
    name: String,
    title: String,
    description: String,
    role: AgentSlashCommandRoleV1,
    available: bool,
    depth: AgentResearchDepthV1,
    requires_subject: bool,
    implicit_primary: Option<String>,
}

impl AgentSlashCommandV1 {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    /// Creates one bounded catalog entry.
    pub const fn new(
        name: String,
        title: String,
        description: String,
        role: AgentSlashCommandRoleV1,
        available: bool,
        depth: AgentResearchDepthV1,
        requires_subject: bool,
        implicit_primary: Option<String>,
    ) -> Self {
        Self {
            name,
            title,
            description,
            role,
            available,
            depth,
            requires_subject,
            implicit_primary,
        }
    }
}

/// Versioned immutable command catalog.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AgentSlashCommandsResponseV1 {
    protocol_version: ProtocolVersion,
    catalog_version: u16,
    commands: Vec<AgentSlashCommandV1>,
}

impl AgentSlashCommandsResponseV1 {
    #[must_use]
    /// Creates the current immutable catalog response.
    pub const fn new(commands: Vec<AgentSlashCommandV1>) -> Self {
        Self {
            protocol_version: ProtocolVersion::CURRENT,
            catalog_version: 1,
            commands,
        }
    }
}
