use a3_domain::{AgentPermissionMode, AgentPermissionRevision, AgentPermissionSettings};
use std::{error::Error, fmt, future::Future, pin::Pin};

/// Owned storage operation for global agent permissions.
pub type AgentPermissionFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AgentPermissionSettings, AgentPermissionStoreFailure>>
            + Send
            + 'a,
    >,
>;

/// Narrow persistent boundary; ordinary workspace text cannot contribute permission settings.
pub trait AgentPermissionStore: fmt::Debug + Send + Sync {
    /// Loads the durable current permission snapshot.
    fn load_agent_permissions(&self) -> AgentPermissionFuture<'_>;
    /// Appends a user-selected mode only if the visible revision is still current.
    fn update_agent_permissions(
        &self,
        expected: AgentPermissionRevision,
        mode: AgentPermissionMode,
    ) -> AgentPermissionFuture<'_>;
}

/// Bounded failures that never expose database contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPermissionStoreFailure {
    /// Settings could not be read or committed.
    Unavailable,
    /// Durable settings are invalid; no machine authority may be inferred.
    InvalidStoredData,
    /// Another user selection already advanced the settings.
    Conflict,
}
impl fmt::Display for AgentPermissionStoreFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "agent permissions unavailable",
            Self::InvalidStoredData => "invalid stored agent permissions",
            Self::Conflict => "agent permissions changed",
        })
    }
}
impl Error for AgentPermissionStoreFailure {}
