use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Durable non-sensitive layout preferences for the Agent workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentWorkspaceLayout {
    session_rail_width: u16,
    inspector_width: u16,
    session_rail_collapsed: bool,
    inspector_collapsed: bool,
}

impl AgentWorkspaceLayout {
    /// Default desktop workspace layout.
    pub const DEFAULT: Self = Self {
        session_rail_width: 264,
        inspector_width: 400,
        session_rail_collapsed: false,
        inspector_collapsed: false,
    };

    /// Validates the desktop layout bounds recorded by ADR-0033.
    pub const fn new(
        session_rail_width: u16,
        inspector_width: u16,
        session_rail_collapsed: bool,
        inspector_collapsed: bool,
    ) -> Result<Self, UiPreferencesError> {
        if session_rail_width < 220
            || session_rail_width > 360
            || inspector_width < 320
            || inspector_width > 640
        {
            Err(UiPreferencesError::InvalidLayout)
        } else {
            Ok(Self {
                session_rail_width,
                inspector_width,
                session_rail_collapsed,
                inspector_collapsed,
            })
        }
    }

    /// Returns the preferred session-rail width.
    #[must_use]
    pub const fn session_rail_width(self) -> u16 {
        self.session_rail_width
    }
    /// Returns the preferred inspector width.
    #[must_use]
    pub const fn inspector_width(self) -> u16 {
        self.inspector_width
    }
    /// Returns whether the rail is collapsed on a wide viewport.
    #[must_use]
    pub const fn session_rail_collapsed(self) -> bool {
        self.session_rail_collapsed
    }
    /// Returns whether the inspector is collapsed on a wide viewport.
    #[must_use]
    pub const fn inspector_collapsed(self) -> bool {
        self.inspector_collapsed
    }
}

/// Monotone global UI-preference store version; zero denotes an empty store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UiPreferencesStoreVersion(u64);

impl UiPreferencesStoreVersion {
    /// Empty store version.
    pub const EMPTY: Self = Self(0);

    /// Reconstructs one locally representable version.
    pub const fn new(value: u64) -> Result<Self, UiPreferencesError> {
        if value > i64::MAX as u64 {
            Err(UiPreferencesError::InvalidVersion)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the numeric store representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Stored layout paired with its optimistic revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredUiPreferences {
    version: UiPreferencesStoreVersion,
    agent_workspace: AgentWorkspaceLayout,
}

impl StoredUiPreferences {
    /// Creates one validated snapshot projection.
    #[must_use]
    pub const fn new(
        version: UiPreferencesStoreVersion,
        agent_workspace: AgentWorkspaceLayout,
    ) -> Self {
        Self {
            version,
            agent_workspace,
        }
    }
    /// Returns the optimistic revision.
    #[must_use]
    pub const fn version(self) -> UiPreferencesStoreVersion {
        self.version
    }
    /// Returns the Agent workspace layout.
    #[must_use]
    pub const fn agent_workspace(self) -> AgentWorkspaceLayout {
        self.agent_workspace
    }
}

/// Owned asynchronous UI-preference storage operation.
pub type UiPreferencesStoreFuture<'a> =
    Pin<Box<dyn Future<Output = Result<StoredUiPreferences, UiPreferencesError>> + Send + 'a>>;

/// Global content-free UI-preference storage boundary.
pub trait UiPreferencesStore: fmt::Debug + Send + Sync {
    /// Loads the latest snapshot or the explicit default at version zero.
    fn load(&self) -> UiPreferencesStoreFuture<'_>;
    /// Compare-and-appends one complete snapshot.
    fn append(
        &self,
        expected: UiPreferencesStoreVersion,
        layout: AgentWorkspaceLayout,
    ) -> UiPreferencesStoreFuture<'_>;
}

/// Stable UI-preference validation or persistence failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiPreferencesError {
    /// A pane width was outside the accepted bounds.
    InvalidLayout,
    /// A revision exceeded the local database representation.
    InvalidVersion,
    /// Another writer committed a snapshot first.
    Conflict,
    /// Local persistence was unavailable or invalid.
    Unavailable,
}

impl fmt::Display for UiPreferencesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLayout => "Agent workspace layout is invalid",
            Self::InvalidVersion => "UI preferences version is invalid",
            Self::Conflict => "UI preferences changed",
            Self::Unavailable => "UI preferences are unavailable",
        })
    }
}

impl Error for UiPreferencesError {}

#[cfg(test)]
mod tests {
    use super::AgentWorkspaceLayout;

    #[test]
    fn layout_enforces_the_accessible_workspace_bounds() {
        assert!(AgentWorkspaceLayout::new(220, 320, false, false).is_ok());
        assert!(AgentWorkspaceLayout::new(219, 400, false, false).is_err());
        assert!(AgentWorkspaceLayout::new(264, 641, false, false).is_err());
    }
}
