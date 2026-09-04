use a3_domain::{
    AgentResearchDepth, AgentSession, AgentSessionEntry, AgentSessionId, AgentSessionRevision,
    AgentSessionSequence, AgentSessionState, ProjectIdentity, SlashCommand,
    SlashCommandCatalogVersion, SlashCommandInvocation, SlashCommandLens,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Owned asynchronous Agent-session storage operation.
pub type AgentSessionStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, AgentSessionStoreFailure>> + Send + 'a>>;

/// Bounded project-local session listing query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionListQuery {
    search: Option<String>,
    include_archived: bool,
    before_updated_at_unix_millis: Option<u64>,
    limit: u16,
}

impl AgentSessionListQuery {
    /// Creates a query capped at fifty session summaries.
    pub fn new(
        search: Option<String>,
        include_archived: bool,
        before_updated_at_unix_millis: Option<u64>,
        limit: u16,
    ) -> Result<Self, AgentSessionStoreFailure> {
        if limit == 0 || limit > 50 {
            return Err(AgentSessionStoreFailure::InvalidInput);
        }
        let search = search
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if search.as_ref().is_some_and(|value| value.len() > 256) {
            return Err(AgentSessionStoreFailure::InvalidInput);
        }
        Ok(Self {
            search,
            include_archived,
            before_updated_at_unix_millis,
            limit,
        })
    }

    /// Returns the optional normalized title query.
    #[must_use]
    pub fn search(&self) -> Option<&str> {
        self.search.as_deref()
    }
    /// Returns whether archived sessions are included.
    #[must_use]
    pub const fn include_archived(&self) -> bool {
        self.include_archived
    }
    /// Returns the exclusive updated-time cursor.
    #[must_use]
    pub const fn before_updated_at_unix_millis(&self) -> Option<u64> {
        self.before_updated_at_unix_millis
    }
    /// Returns the maximum number of summaries.
    #[must_use]
    pub const fn limit(&self) -> u16 {
        self.limit
    }
}

/// One bounded page of session summaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionPage {
    sessions: Vec<AgentSession>,
    has_more: bool,
}

impl AgentSessionPage {
    /// Creates a storage-validated bounded page.
    pub fn new(
        sessions: Vec<AgentSession>,
        has_more: bool,
    ) -> Result<Self, AgentSessionStoreFailure> {
        if sessions.len() > 50 {
            return Err(AgentSessionStoreFailure::InvalidStoredData);
        }
        Ok(Self { sessions, has_more })
    }

    /// Returns summaries newest first.
    #[must_use]
    pub fn sessions(&self) -> &[AgentSession] {
        &self.sessions
    }
    /// Returns whether an older page exists.
    #[must_use]
    pub const fn has_more(&self) -> bool {
        self.has_more
    }
}

/// One session with a bounded contiguous conversation page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionDetail {
    session: AgentSession,
    entries: Vec<AgentSessionEntry>,
    has_older_entries: bool,
}

impl AgentSessionDetail {
    /// Creates a validated bounded detail projection.
    pub fn new(
        session: AgentSession,
        entries: Vec<AgentSessionEntry>,
        has_older_entries: bool,
    ) -> Result<Self, AgentSessionStoreFailure> {
        if entries.len() > 128
            || entries
                .iter()
                .any(|entry| entry.session_id() != session.id())
            || entries
                .windows(2)
                .any(|pair| pair[0].sequence() >= pair[1].sequence())
        {
            return Err(AgentSessionStoreFailure::InvalidStoredData);
        }
        Ok(Self {
            session,
            entries,
            has_older_entries,
        })
    }

    /// Returns the latest session projection.
    #[must_use]
    pub const fn session(&self) -> &AgentSession {
        &self.session
    }
    /// Returns entries in ascending sequence order.
    #[must_use]
    pub fn entries(&self) -> &[AgentSessionEntry] {
        &self.entries
    }
    /// Returns whether an earlier page exists.
    #[must_use]
    pub const fn has_older_entries(&self) -> bool {
        self.has_older_entries
    }
}

/// Persisted user-facing command metadata for one session entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionCommandPresentation {
    sequence: AgentSessionSequence,
    catalog_version: SlashCommandCatalogVersion,
    primary: SlashCommand,
    lenses: Vec<SlashCommandLens>,
    depth: AgentResearchDepth,
}

impl AgentSessionCommandPresentation {
    /// Revalidates a bounded command presentation reconstructed by a store adapter.
    pub fn restore(
        sequence: AgentSessionSequence,
        catalog_version: SlashCommandCatalogVersion,
        primary: SlashCommand,
        lenses: Vec<SlashCommandLens>,
        depth: AgentResearchDepth,
    ) -> Result<Self, AgentSessionStoreFailure> {
        if lenses.len() > 2
            || lenses.windows(2).any(|pair| pair[0] == pair[1])
            || depth
                != if lenses.is_empty() {
                    primary.depth()
                } else {
                    AgentResearchDepth::Thorough
                }
        {
            return Err(AgentSessionStoreFailure::InvalidStoredData);
        }
        Ok(Self {
            sequence,
            catalog_version,
            primary,
            lenses,
            depth,
        })
    }

    /// Returns the user-entry sequence.
    #[must_use]
    pub const fn sequence(&self) -> AgentSessionSequence {
        self.sequence
    }

    /// Returns the immutable catalog version.
    #[must_use]
    pub const fn catalog_version(&self) -> SlashCommandCatalogVersion {
        self.catalog_version
    }

    /// Returns the primary command.
    #[must_use]
    pub const fn primary(&self) -> SlashCommand {
        self.primary
    }

    /// Returns the ordered specialist lenses.
    #[must_use]
    pub fn lenses(&self) -> &[SlashCommandLens] {
        &self.lenses
    }

    /// Returns the Core-owned effective depth.
    #[must_use]
    pub const fn depth(&self) -> AgentResearchDepth {
        self.depth
    }
}

/// Persistence boundary for project-local conversation presentation data.
pub trait AgentSessionStore: fmt::Debug + Send + Sync {
    /// Creates revision one, optionally with its sequence-one entry, atomically.
    fn create_session<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session: &'a AgentSession,
        first_entry: Option<&'a AgentSessionEntry>,
        command: Option<&'a SlashCommandInvocation>,
    ) -> AgentSessionStoreFuture<'a, ()>;

    /// Appends the immediate session revision and optional immediate entry atomically.
    fn append_session_revision<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        expected_revision: AgentSessionRevision,
        session: &'a AgentSession,
        entry: Option<&'a AgentSessionEntry>,
        command: Option<&'a SlashCommandInvocation>,
    ) -> AgentSessionStoreFuture<'a, ()>;

    /// Reads project-local session summaries.
    fn list_sessions<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        query: &'a AgentSessionListQuery,
    ) -> AgentSessionStoreFuture<'a, AgentSessionPage>;

    /// Reads one latest session and a bounded tail ending before the optional sequence.
    fn load_session<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session_id: AgentSessionId,
        before_sequence: Option<u64>,
        limit: u16,
    ) -> AgentSessionStoreFuture<'a, Option<AgentSessionDetail>>;

    /// Loads the bounded command metadata belonging to one visible session page.
    fn load_session_commands<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session_id: AgentSessionId,
        before_sequence: Option<u64>,
        limit: u16,
    ) -> AgentSessionStoreFuture<'a, Vec<AgentSessionCommandPresentation>>;

    /// Removes presentation entries after an append-only tombstone revision was committed.
    fn delete_presentation<'a>(
        &'a self,
        project: &'a ProjectIdentity,
        session_id: AgentSessionId,
        expected_revision: AgentSessionRevision,
        tombstone: &'a AgentSession,
    ) -> AgentSessionStoreFuture<'a, ()>;
}

/// Stable session persistence failure without SQL or content details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSessionStoreFailure {
    /// Input violated an application bound.
    InvalidInput,
    /// Stored rows violated the closed domain model.
    InvalidStoredData,
    /// Another writer committed the session first.
    Conflict,
    /// Local storage was unavailable.
    Unavailable,
}

impl fmt::Display for AgentSessionStoreFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInput => "Agent session request is invalid",
            Self::InvalidStoredData => "Agent session storage contains invalid data",
            Self::Conflict => "Agent session revision changed",
            Self::Unavailable => "Agent session storage is unavailable",
        })
    }
}

impl Error for AgentSessionStoreFailure {}

/// Pure mode-policy guard shared by IPC and execution orchestration.
pub fn validate_agent_session_transition(
    current: &AgentSession,
    next: &AgentSession,
) -> Result<(), AgentSessionStoreFailure> {
    if current.id() != next.id()
        || next.revision().get() != current.revision().get().saturating_add(1)
        || next.created_at() != current.created_at()
        || next.updated_at() < current.updated_at()
        || (current.presentation_deleted() && !next.presentation_deleted())
    {
        return Err(AgentSessionStoreFailure::InvalidInput);
    }
    if current.mode() != next.mode() && !current.mode().can_transition_to(next.mode()) {
        return Err(AgentSessionStoreFailure::InvalidInput);
    }
    if current.state() == AgentSessionState::Archived && next.state() == AgentSessionState::Running
    {
        return Err(AgentSessionStoreFailure::InvalidInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AgentSessionStoreFailure, validate_agent_session_transition};
    use a3_domain::{
        AgentSession, AgentSessionId, AgentSessionMode, AgentSessionRevision, AgentSessionState,
        AgentSessionTimestamp, AgentSessionTitle,
    };

    fn session(revision: u64, mode: AgentSessionMode) -> AgentSession {
        AgentSession::from_parts(
            AgentSessionId::from_bytes([1; 32]),
            AgentSessionRevision::new(revision).unwrap_or(AgentSessionRevision::INITIAL),
            AgentSessionTitle::try_from_string("Task".to_owned())
                .unwrap_or_else(|_| unreachable!()),
            mode,
            AgentSessionState::Running,
            AgentSessionTimestamp::from_unix_millis(1).unwrap_or_else(|_| unreachable!()),
            AgentSessionTimestamp::from_unix_millis(revision).unwrap_or_else(|_| unreachable!()),
            None,
            None,
            None,
            false,
        )
    }

    #[test]
    fn reverse_mode_transition_is_rejected() {
        assert_eq!(
            validate_agent_session_transition(
                &session(1, AgentSessionMode::Agent),
                &session(2, AgentSessionMode::Ask),
            ),
            Err(AgentSessionStoreFailure::InvalidInput)
        );
    }
}
